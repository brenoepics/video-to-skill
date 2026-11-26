use std::path::Path;

use super::{SavedDacl, SecurityState};

#[test]
fn an_inheritable_null_dacl_is_not_mutated() {
    let state = SecurityState {
        dacl: SavedDacl::Null,
        protected: false,
    };

    let error = state.ensure_restorable(Path::new("workspace")).unwrap_err();

    assert!(error.to_string().contains("inheritable NULL DACL"));
}

#[test]
fn a_protected_null_dacl_can_be_preserved() {
    let state = SecurityState {
        dacl: SavedDacl::Null,
        protected: true,
    };

    state.ensure_restorable(Path::new("workspace")).unwrap();
}
