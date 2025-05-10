use hyper::Method;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MethodKind {
    Get = 0,
    Post = 1,
    Put = 2,
    Delete = 3,
    Patch = 4,
    Head = 5,
    Options = 6,
    Trace = 7,
    Connect = 8,
}

impl MethodKind {
    pub fn from_hyper(method: &hyper::Method) -> Self {
        match method {
            &Method::GET => MethodKind::Get,
            &Method::POST => MethodKind::Post,
            &Method::PUT => MethodKind::Put,
            &Method::DELETE => MethodKind::Delete,
            &Method::PATCH => MethodKind::Patch,
            &Method::HEAD => MethodKind::Head,
            &Method::OPTIONS => MethodKind::Options,
            &Method::TRACE => MethodKind::Trace,
            &Method::CONNECT => MethodKind::Connect,
            _ => unreachable!(),
        }
    }
}
