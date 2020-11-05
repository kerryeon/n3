mod error;
mod host;

pub use n3_machine_ffi::{
    JobId, Machine, MachineId, Program, Query, Result as MachineResult, SignalHandler,
};

pub use self::error::{Error, MachineError, Result};
pub use self::host::{Generator, HostMachine};
