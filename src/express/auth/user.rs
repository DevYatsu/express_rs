#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthLevel {
    Guest,
    User,
    Admin,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub token: String,
    pub level: AuthLevel,
}
