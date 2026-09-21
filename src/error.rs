//! Error type for the mailspace crate.
//!
//! The same type is used across both halves during the crate split, so it is
//! re-exported rather than duplicated. Phase two of the split gives each
//! repository its own copy.

pub use vivi_mail::VivariumError;
