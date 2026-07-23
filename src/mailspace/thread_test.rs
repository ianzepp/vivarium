use super::*;

#[test]
fn strips_stacked_reply_prefixes() {
    assert_eq!(strip_reply_prefix("Re: Re[2]: status"), "status");
}

#[test]
fn reply_prefix_is_case_insensitive() {
    assert_eq!(strip_reply_prefix("rE: Status"), "status");
}
