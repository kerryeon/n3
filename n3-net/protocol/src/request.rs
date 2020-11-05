use serde::{Deserialize, Serialize};

use n3_machine_ffi::{JobId, MachineId, ProgramVec, Query};

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Load {
        job: JobId,
        query: Vec<Query>,
    },
    Spawn {
        job: JobId,
        id_primaries: Vec<MachineId>,
        id_local: MachineId,
        id_world: MachineId,
        program: ProgramVec,
        command: String,
    },
    Join {
        job: JobId,
    },
    Terminate {
        job: JobId,
    },
}
