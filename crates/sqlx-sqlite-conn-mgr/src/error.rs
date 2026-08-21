//! Error types for sqlx-sqlite-conn-mgr

use thiserror::Error;

/// Errors that may occur when working with sqlx-sqlite-conn-mgr
#[derive(Error, Debug)]
pub enum Error {
   /// IO error when accessing database files. Standard library IO errors
   /// are converted to this variant.
   #[error("IO error: {0}")]
   Io(#[from] std::io::Error),

   /// Error from the sqlx library. Standard sqlx errors are converted to this variant
   #[error("Sqlx error: {0}")]
   Sqlx(#[from] sqlx::Error),

   /// Migration error from the sqlx migrate framework
   #[error("Migration error: {0}")]
   Migration(#[from] sqlx::migrate::MigrateError),

   /// Database has been closed and cannot be used
   #[error("Database has been closed")]
   DatabaseClosed,

   /// Cannot attach a database as read-write to a read-only connection
   #[error("Cannot attach database as read-write to a read-only connection")]
   CannotAttachReadWriteToReader,

   /// Invalid schema name provided for attached database. See
   /// `attached::is_valid_schema_name` for the authoritative rule set this message
   /// must stay in sync with.
   #[error(
      "Invalid schema name '{0}': must be non-empty, contain only alphanumeric characters and underscores, not start with a digit, be at most 64 bytes long, and not be the reserved name 'main' or 'temp' (case-insensitive)"
   )]
   InvalidSchemaName(String),

   /// Attempted to attach the same database multiple times
   #[error(
      "Database '{0}' appears multiple times in attached database list (would cause deadlock)"
   )]
   DuplicateAttachedDatabase(String),

   /// Invalid name for a scalar function. See `functions::validate_name` for the rule set
   /// this message must match.
   #[error(
      "Invalid function name '{0}': must be non-empty, at most 255 bytes long, and contain no interior NUL byte"
   )]
   InvalidFunctionName(String),

   /// Invalid argument count for a scalar function. In SQLite, a negative arity means a
   /// variadic function. This crate does not offer that option. The upper bound is the
   /// linked library's `SQLITE_MAX_FUNCTION_ARG`, enforced by the probe as
   /// `Error::FunctionRefused`.
   #[error("Invalid function arity {0}: must not be negative")]
   InvalidFunctionArity(i32),

   /// A scalar function with this name and argument count is already registered. This
   /// crate compares names case-insensitively, to match how SQLite resolves them.
   /// Without this check, a second registration replaces the first instead of the two
   /// coexisting.
   #[error("A function named '{name}' taking {arity} argument(s) is already registered")]
   DuplicateFunction { name: String, arity: i32 },

   /// The linked SQLite library already defines this name as a built-in with the same
   /// argument count, or as a variadic built-in. SQLite silently replaces a built-in with
   /// an application-defined function, so every statement on the connection that names the
   /// built-in reaches the handler instead, including one inside an existing view, index,
   /// or constraint. See `functions::shadows_builtin`.
   #[error(
      "A function named '{name}' taking {arity} argument(s) replaces a SQLite built-in function. Choose a name no built-in uses."
   )]
   ShadowsBuiltinFunction { name: String, arity: i32 },

   /// The in-memory connection that checks a registration failed to open or query, so
   /// this crate cannot tell whether the linked library accepts the function. The failure
   /// belongs to the probe, not to the function. See `functions::probe`.
   #[error(
      "Cannot check whether SQLite accepts a function named '{name}': the probe failed: {reason}"
   )]
   ProbeConnectionFailed { name: String, reason: String },

   /// The linked SQLite library refused the registration. SQLite validates the name, the
   /// argument count, and the encoding flags. The library's own limits decide the
   /// outcome: an arity above its `SQLITE_MAX_FUNCTION_ARG` fails here. The `reason`
   /// field gives the message SQLite returned.
   #[error("SQLite refused a function named '{name}' taking {arity} argument(s): {reason}")]
   FunctionRefused {
      name: String,
      arity: i32,
      reason: String,
   },

   /// Two attached-database specs used the same schema alias. Compared
   /// case-insensitively, matching SQLite's own schema namespace - a spec named `"x"`
   /// and one named `"X"` collide at `ATTACH` even though they compare unequal as
   /// plain strings.
   #[error("Schema name '{0}' is used by more than one attached database")]
   DuplicateSchemaName(String),
}
