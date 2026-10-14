//! Narrow process capability. Core cannot spawn processes or elevate privilege.
use crate::Result;
use std::{ffi::OsString, future::Future, time::Duration};
#[derive(Clone, Debug)]
pub enum Program {
    NetworkManager,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub program: Program,
    pub arguments: Vec<OsString>,
    pub timeout: Duration,
}
#[derive(Clone, Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
}
pub trait ProcessRunner: Send + Sync {
    fn run(&self, request: Request) -> impl Future<Output = Result<Output>> + Send;
}
