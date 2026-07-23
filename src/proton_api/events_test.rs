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
