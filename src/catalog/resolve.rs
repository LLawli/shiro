//! Walking an argument path into the merged tree.
//!
//! Consumes arguments while they name children. What is left when the walk
//! stops is either the node the caller meant, or the error that names how far
//! the path got and what was available there.
