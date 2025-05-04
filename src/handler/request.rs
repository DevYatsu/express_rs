use hyper::{Request as HRequest, body::Incoming};

pub type Request = HRequest<Incoming>;
