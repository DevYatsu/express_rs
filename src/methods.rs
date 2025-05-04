#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Head,
    Delete,
    Options,
    Trace,
    Copy,
    Lock,
    Mkcol,
    Move,
    Purge,
    Propfind,
    Proppatch,
    Unlock,
    Report,
    Mkactivity,
    Checkout,
    Merge,
    MSearch,
    Notify,
    Subscribe,
    Unsubscribe,
    Patch,
    Search,
    Connect,
}

use std::str::FromStr;

impl FromStr for Method {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "get" => Ok(Method::Get),
            "post" => Ok(Method::Post),
            "put" => Ok(Method::Put),
            "head" => Ok(Method::Head),
            "delete" => Ok(Method::Delete),
            "options" => Ok(Method::Options),
            "trace" => Ok(Method::Trace),
            "copy" => Ok(Method::Copy),
            "lock" => Ok(Method::Lock),
            "mkcol" => Ok(Method::Mkcol),
            "move" => Ok(Method::Move),
            "purge" => Ok(Method::Purge),
            "propfind" => Ok(Method::Propfind),
            "proppatch" => Ok(Method::Proppatch),
            "unlock" => Ok(Method::Unlock),
            "report" => Ok(Method::Report),
            "mkactivity" => Ok(Method::Mkactivity),
            "checkout" => Ok(Method::Checkout),
            "merge" => Ok(Method::Merge),
            "m-search" => Ok(Method::MSearch),
            "notify" => Ok(Method::Notify),
            "subscribe" => Ok(Method::Subscribe),
            "unsubscribe" => Ok(Method::Unsubscribe),
            "patch" => Ok(Method::Patch),
            "search" => Ok(Method::Search),
            "connect" => Ok(Method::Connect),
            _ => Err(()),
        }
    }
}

use std::fmt;

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let method_str = match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Head => "HEAD",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Trace => "TRACE",
            Method::Copy => "COPY",
            Method::Lock => "LOCK",
            Method::Mkcol => "MKCOL",
            Method::Move => "MOVE",
            Method::Purge => "PURGE",
            Method::Propfind => "PROPFIND",
            Method::Proppatch => "PROPPATCH",
            Method::Unlock => "UNLOCK",
            Method::Report => "REPORT",
            Method::Mkactivity => "MKACTIVITY",
            Method::Checkout => "CHECKOUT",
            Method::Merge => "MERGE",
            Method::MSearch => "M-SEARCH",
            Method::Notify => "NOTIFY",
            Method::Subscribe => "SUBSCRIBE",
            Method::Unsubscribe => "UNSUBSCRIBE",
            Method::Patch => "PATCH",
            Method::Search => "SEARCH",
            Method::Connect => "CONNECT",
        };
        write!(f, "{method_str}")
    }
}
