use serde::{Deserialize, Deserializer, Serialize};

use super::{ProtonApiClient, ProtonMessage, ProtonSession};
use crate::error::VivariumError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ProtonEvent {
    #[serde(rename = "EventID", default)]
    pub event_id: String,
    #[serde(rename = "Refresh", default)]
    pub refresh: u8,
    #[serde(rename = "Messages", default)]
    pub messages: Vec<ProtonMessageEvent>,
}

impl ProtonEvent {
    #[must_use] 
    pub fn requires_mail_refresh(&self) -> bool {
        self.refresh & REFRESH_MAIL != 0 || self.refresh == REFRESH_ALL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ProtonMessageEvent {
    #[serde(rename = "ID", default)]
    pub id: String,
    #[serde(rename = "Action", default, deserialize_with = "deserialize_action")]
    pub action: ProtonEventAction,
    #[serde(rename = "Message", default)]
    pub message: Option<ProtonMessage>,
}

fn deserialize_action<'de, D>(deserializer: D) -> Result<ProtonEventAction, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u8::deserialize(deserializer)?;
    Ok(match value {
        0 => ProtonEventAction::Delete,
        1 => ProtonEventAction::Create,
        3 => ProtonEventAction::UpdateFlags,
        _ => ProtonEventAction::Update,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProtonEventAction {
    #[serde(rename = "0")]
    #[default]
    Delete,
    #[serde(rename = "1")]
    Create,
    #[serde(rename = "2")]
    Update,
    #[serde(rename = "3")]
    UpdateFlags,
}

#[derive(Debug, Deserialize)]
pub(super) struct LatestEventResponse {
    #[serde(rename = "Event")]
    pub event: ProtonEvent,
}

#[derive(Debug, Deserialize)]
pub(super) struct EventResponse {
    #[serde(rename = "Event")]
    pub event: ProtonEvent,
    #[serde(rename = "More", default)]
    pub more: bool,
}

const REFRESH_MAIL: u8 = 1;
const REFRESH_ALL: u8 = 255;

impl ProtonApiClient {
    /// Fetches the latest event ID from the Proton API.
    ///
    /// # Errors
    /// Returns an error if the API call fails or the session cannot be refreshed.
    pub async fn latest_event_id(
        &self,
        session: &ProtonSession,
    ) -> Result<(ProtonSession, String), VivariumError> {
        match self.latest_event_with_session(session).await? {
            EventAttempt::Ok(event) => Ok((session.clone(), event.event_id)),
            EventAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let event = match self.latest_event_with_session(&refreshed).await? {
                    EventAttempt::Ok(event) => event,
                    EventAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API latest event request was unauthorized after session refresh"
                                .into(),
                        ));
                    }
                };
                Ok((refreshed, event.event_id))
            }
        }
    }

    /// Fetches events from the Proton API starting from the given event ID.
    ///
    /// Returns the (possibly refreshed) session, the event, and whether more events are available.
    ///
    /// # Errors
    /// Returns an error if the API call fails or the session cannot be refreshed.
    pub async fn event(
        &self,
        session: &ProtonSession,
        event_id: &str,
    ) -> Result<(ProtonSession, ProtonEvent, bool), VivariumError> {
        match self.event_with_session(session, event_id).await? {
            EventPageAttempt::Ok(response) => Ok((session.clone(), response.event, response.more)),
            EventPageAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let response = match self.event_with_session(&refreshed, event_id).await? {
                    EventPageAttempt::Ok(response) => response,
                    EventPageAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API event request was unauthorized after session refresh"
                                .into(),
                        ));
                    }
                };
                Ok((refreshed, response.event, response.more))
            }
        }
    }

    async fn latest_event_with_session(
        &self,
        session: &ProtonSession,
    ) -> Result<EventAttempt, VivariumError> {
        let Some(response) = self
            .get_authenticated::<LatestEventResponse>("/core/v4/events/latest", session)
            .await?
        else {
            return Ok(EventAttempt::Unauthorized);
        };
        Ok(EventAttempt::Ok(response.event))
    }

    async fn event_with_session(
        &self,
        session: &ProtonSession,
        event_id: &str,
    ) -> Result<EventPageAttempt, VivariumError> {
        let path = format!("/core/v4/events/{event_id}");
        let Some(response) = self
            .get_authenticated::<EventResponse>(&path, session)
            .await?
        else {
            return Ok(EventPageAttempt::Unauthorized);
        };
        Ok(EventPageAttempt::Ok(response))
    }
}

enum EventAttempt {
    Ok(ProtonEvent),
    Unauthorized,
}

enum EventPageAttempt {
    Ok(EventResponse),
    Unauthorized,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_response_parses_message_actions() {
        let response: EventResponse = serde_json::from_str(
            r#"{"Event":{"EventID":"event-2","Messages":[{"ID":"proton-id","Action":1,"Message":{"ID":"proton-id","Subject":"subject","LabelIDs":["0"]}}]},"More":false}"#,
        )
        .unwrap();

        assert!(!response.more);
        assert_eq!(response.event.event_id, "event-2");
        assert_eq!(response.event.messages[0].id, "proton-id");
        assert_eq!(response.event.messages[0].action, ProtonEventAction::Create);
        assert_eq!(
            response.event.messages[0].message.as_ref().unwrap().subject,
            "subject"
        );
    }
}
