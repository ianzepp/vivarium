use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::Serialize;

use super::kind::{effective_kind, matches_kind};
use super::{Mailspace, canonical_local_role};
use crate::error::VivariumError;
use crate::storage::{MailspaceEvent, StoredMessageView};

#[derive(Debug, Clone, Default)]
pub struct DumpFilters {
    pub for_identity: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub participant: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub since: Option<String>,
    pub before: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MailDumpRequest {
    pub folder: String,
    pub kind: Option<String>,
    pub filters: DumpFilters,
}

#[derive(Debug, Clone)]
pub struct TaskDumpRequest {
    pub status: TaskDumpStatus,
    pub open_role: String,
    pub kind: String,
    pub filters: DumpFilters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskDumpStatus {
    Open,
    Done,
    All,
}

#[derive(Debug, Clone, Serialize)]
pub struct DumpRecord {
    pub handle: String,
    pub message_id: String,
    pub account: String,
    pub role: String,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub date: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub events: Vec<MailspaceEvent>,
}

struct PreparedFilters {
    account: Option<String>,
    from: Option<String>,
    to: Option<String>,
    participant: Option<ParticipantFilter>,
    subject: Option<String>,
    body: Option<String>,
    window: DumpWindow,
}

struct ParticipantFilter {
    identity: Option<String>,
    text: String,
}

#[derive(Default)]
struct DumpWindow {
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
}

impl Mailspace {
    pub fn dump_mail(&self, request: MailDumpRequest) -> Result<Vec<DumpRecord>, VivariumError> {
        let roles = mail_roles(&request.folder)?;
        self.dump_records(&roles, None, request.kind.as_deref(), request.filters)
    }

    pub fn dump_tasks(&self, request: TaskDumpRequest) -> Result<Vec<DumpRecord>, VivariumError> {
        let roles = status_roles(&request.open_role, request.status);
        self.dump_records(
            &roles,
            Some((request.status, request.open_role.as_str())),
            Some(&request.kind),
            request.filters,
        )
    }

    fn dump_records(
        &self,
        roles: &[String],
        status: Option<(TaskDumpStatus, &str)>,
        kind: Option<&str>,
        filters: DumpFilters,
    ) -> Result<Vec<DumpRecord>, VivariumError> {
        let filters = self.prepare_filters(filters)?;
        let storage = self.storage()?;
        let mut records = Vec::new();
        for view in storage.list_messages()? {
            if !roles.iter().any(|role| role == &view.local_role) {
                continue;
            }
            if let Some(record) = self.filtered_record(&storage, view, status, kind, &filters)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn filtered_record(
        &self,
        storage: &crate::storage::Storage,
        view: StoredMessageView,
        status: Option<(TaskDumpStatus, &str)>,
        kind: Option<&str>,
        filters: &PreparedFilters,
    ) -> Result<Option<DumpRecord>, VivariumError> {
        let data = storage.read_message(&view.message_id)?;
        let events = storage.list_mailspace_events(&view.message_id)?;
        let body = text_body(&data);
        if kind.is_some_and(|kind| !matches_kind(&view.local_role, &data, &events, kind)) {
            return Ok(None);
        }
        let message_kind = effective_kind(&view.local_role, &data, &events);
        if !matches_filters(&view, &body, filters) {
            return Ok(None);
        }
        Ok(Some(DumpRecord {
            events,
            handle: view.handle,
            message_id: view.message_id,
            account: view.account,
            role: view.local_role.clone(),
            kind: message_kind,
            status: status.and_then(|(_, open_role)| status_for_role(&view.local_role, open_role)),
            date: view.date,
            from: view.from_addr,
            to: view.to_addr,
            cc: view.cc_addr,
            subject: view.subject,
            body,
        }))
    }

    fn prepare_filters(&self, filters: DumpFilters) -> Result<PreparedFilters, VivariumError> {
        Ok(PreparedFilters {
            account: filters
                .for_identity
                .as_deref()
                .map(|identity| self.resolve_identity(identity))
                .transpose()?,
            from: filters.from.map(normalize_filter),
            to: filters.to.map(normalize_filter),
            participant: self.participant_filter(filters.participant.as_deref())?,
            subject: filters.subject.map(normalize_filter),
            body: filters.body.map(normalize_filter),
            window: DumpWindow::parse(filters.since.as_deref(), filters.before.as_deref())?,
        })
    }

    fn participant_filter(
        &self,
        participant: Option<&str>,
    ) -> Result<Option<ParticipantFilter>, VivariumError> {
        let Some(value) = participant else {
            return Ok(None);
        };
        let identity = self.resolve_identity(value).ok();
        let text = identity
            .as_deref()
            .map(|identity| self.address_for(identity))
            .unwrap_or_else(|| value.to_string());
        Ok(Some(ParticipantFilter {
            identity,
            text: normalize_filter(text),
        }))
    }
}

fn matches_filters(view: &StoredMessageView, body: &str, filters: &PreparedFilters) -> bool {
    filters
        .account
        .as_ref()
        .is_none_or(|account| &view.account == account)
        && filters.window.contains(&view.date)
        && matches_text(&view.from_addr, filters.from.as_deref())
        && matches_recipients(view, filters.to.as_deref())
        && matches_text(&view.subject, filters.subject.as_deref())
        && matches_text(body, filters.body.as_deref())
        && matches_participant(view, filters.participant.as_ref())
}

fn matches_recipients(view: &StoredMessageView, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    matches_text(&view.to_addr, Some(filter)) || matches_text(&view.cc_addr, Some(filter))
}

fn matches_participant(view: &StoredMessageView, filter: Option<&ParticipantFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    filter
        .identity
        .as_ref()
        .is_some_and(|id| &view.account == id)
        || matches_text(&view.from_addr, Some(&filter.text))
        || matches_text(&view.to_addr, Some(&filter.text))
        || matches_text(&view.cc_addr, Some(&filter.text))
        || matches_text(&view.bcc_addr, Some(&filter.text))
}

fn matches_text(value: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|filter| value.to_ascii_lowercase().contains(filter))
}

fn mail_roles(folder: &str) -> Result<Vec<String>, VivariumError> {
    if folder.eq_ignore_ascii_case("all") {
        return Ok(["inbox", "sent", "archive", "trash", "drafts"]
            .into_iter()
            .map(str::to_string)
            .collect());
    }
    canonical_local_role(folder).map(|role| vec![role])
}

fn status_roles(open_role: &str, status: TaskDumpStatus) -> Vec<String> {
    match status {
        TaskDumpStatus::Open => vec![open_role.into()],
        TaskDumpStatus::Done => vec!["done".into()],
        TaskDumpStatus::All => vec![open_role.into(), "done".into()],
    }
}

fn status_for_role(role: &str, open_role: &str) -> Option<String> {
    if role == open_role {
        Some("open".into())
    } else if role == "done" {
        Some("done".into())
    } else {
        None
    }
}

fn text_body(data: &[u8]) -> String {
    mail_parser::MessageParser::default()
        .parse(data)
        .and_then(|parsed| parsed.body_text(0).map(|body| body.to_string()))
        .unwrap_or_default()
}

fn normalize_filter(value: impl AsRef<str>) -> String {
    value.as_ref().to_ascii_lowercase()
}

impl DumpWindow {
    fn parse(since: Option<&str>, before: Option<&str>) -> Result<Self, VivariumError> {
        Ok(Self {
            since: since.map(parse_time_bound).transpose()?,
            before: before.map(parse_time_bound).transpose()?,
        })
    }

    fn contains(&self, raw_date: &str) -> bool {
        let Some(date) = parse_message_date(raw_date) else {
            return self.since.is_none() && self.before.is_none();
        };
        self.since.is_none_or(|since| date >= since)
            && self.before.is_none_or(|before| date < before)
    }
}

fn parse_time_bound(value: &str) -> Result<DateTime<Utc>, VivariumError> {
    if let Some(date) = relative_time_bound(value)? {
        return Ok(date);
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return Ok(date.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        VivariumError::Config(format!(
            "invalid time '{value}', expected RFC3339, YYYY-MM-DD, Nh, Nd, or Nw"
        ))
    })?;
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|date| date.with_timezone(&Utc))
        .ok_or_else(|| VivariumError::Config(format!("ambiguous local date '{value}'")))
}

fn relative_time_bound(value: &str) -> Result<Option<DateTime<Utc>>, VivariumError> {
    let now = Utc::now();
    if let Some(count) = parse_suffix(value, "h")? {
        return Ok(Some(now - Duration::hours(count)));
    }
    if let Some(count) = parse_suffix(value, "d")? {
        return Ok(Some(now - Duration::days(count)));
    }
    if let Some(count) = parse_suffix(value, "w")? {
        return Ok(Some(now - Duration::weeks(count)));
    }
    Ok(None)
}

fn parse_suffix(value: &str, suffix: &str) -> Result<Option<i64>, VivariumError> {
    let Some(number) = value.strip_suffix(suffix) else {
        return Ok(None);
    };
    number.parse::<i64>().map(Some).map_err(|_| {
        VivariumError::Config(format!(
            "invalid relative time '{value}', expected Nh, Nd, or Nw"
        ))
    })
}

fn parse_message_date(raw_date: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw_date)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}
