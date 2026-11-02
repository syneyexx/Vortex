//! Gegenereerde contracten uit `/proto`. Niet met de hand bewerken — wijzig de .proto-bestanden.
//! Kaart: VAULT/15-CONTRACTEN.md.
// VX:W00.11
#![allow(clippy::all, clippy::pedantic, unused_qualifications)]

pub mod common {
    pub mod v1 {
        tonic::include_proto!("vortex.common.v1");
    }
}
pub mod worker {
    pub mod v1 {
        tonic::include_proto!("vortex.worker.v1");
    }
}
pub mod gui {
    pub mod v1 {
        tonic::include_proto!("vortex.gui.v1");
    }
}
pub mod admin {
    pub mod v1 {
        tonic::include_proto!("vortex.admin.v1");
    }
}
pub mod sandbox {
    pub mod v1 {
        tonic::include_proto!("vortex.sandbox.v1");
    }
}
