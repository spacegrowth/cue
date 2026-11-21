//! Landing an item's work: finishing, committing, trees and copies, installing, PRs, and the git underneath.

pub(crate) mod finish;
pub(crate) mod commit;
pub(crate) mod trees;
pub(crate) mod copies;
pub(crate) mod copy_guard;
pub(crate) mod install;
pub(crate) mod pr;
pub(crate) mod tree_files;
pub(crate) mod main_line;
pub(crate) mod unlanded;
pub(crate) mod own_work;
pub(crate) mod diff;
pub(crate) mod git;
pub(crate) mod land_keep;
pub(crate) mod named_copy;
pub(crate) mod remote_item;
pub(crate) mod push_check;
