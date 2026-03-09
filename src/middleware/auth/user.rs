/// Represents the authorization level of an authenticated user.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthLevel {
    /// A guest user.
    Guest,
    /// A regular authenticated user.
    User,
    /// An administrator.
    Admin,
}

/// Represents an authenticated user in the system.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The user's authentication token.
    pub token: String,
    /// The user's authorization level.
    pub level: AuthLevel,
}
