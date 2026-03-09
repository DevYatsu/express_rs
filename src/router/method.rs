use hyper::Method;

/// Represents an HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MethodKind {
    /// The GET method.
    Get = 0,
    /// The POST method.
    Post = 1,
    /// The PUT method.
    Put = 2,
    /// The DELETE method.
    Delete = 3,
    /// The PATCH method.
    Patch = 4,
    /// The HEAD method.
    Head = 5,
    /// The OPTIONS method.
    Options = 6,
    /// The TRACE method.
    Trace = 7,
    /// The CONNECT method.
    Connect = 8,
}

impl MethodKind {
    /// An array containing all supported standard HTTP methods.
    pub const ALL: [MethodKind; 9] = [
        MethodKind::Get,
        MethodKind::Post,
        MethodKind::Put,
        MethodKind::Delete,
        MethodKind::Patch,
        MethodKind::Head,
        MethodKind::Options,
        MethodKind::Trace,
        MethodKind::Connect,
    ];

    /// Converts hyper's method to our internal HTTP method variant.
    pub fn from_hyper(method: &hyper::Method) -> Self {
        match *method {
            Method::GET => MethodKind::Get,
            Method::POST => MethodKind::Post,
            Method::PUT => MethodKind::Put,
            Method::DELETE => MethodKind::Delete,
            Method::PATCH => MethodKind::Patch,
            Method::HEAD => MethodKind::Head,
            Method::OPTIONS => MethodKind::Options,
            Method::TRACE => MethodKind::Trace,
            Method::CONNECT => MethodKind::Connect,
            _ => unreachable!(),
        }
    }

    /// Convert array index back to MethodKind (used by MethodRoutes::iter).
    #[inline]
    pub(crate) fn from_index(i: usize) -> Self {
        match i {
            0 => MethodKind::Get,
            1 => MethodKind::Post,
            2 => MethodKind::Put,
            3 => MethodKind::Delete,
            4 => MethodKind::Patch,
            5 => MethodKind::Head,
            6 => MethodKind::Options,
            7 => MethodKind::Trace,
            8 => MethodKind::Connect,
            _ => unreachable!("invalid MethodKind index: {}", i),
        }
    }
}
