//! Inventory module — phased hardware & software data collection.
//!
//! Uses a staged sync architecture that gradually builds a complete
//! inventory snapshot without overloading boot-time I/O, then switches
//! to incremental mode detecting and recording changes over time.

pub mod hardware;
pub mod software;
pub mod collector;
pub mod diff;
pub mod history;

pub use collector::InventoryCollector;
pub use hardware::HardwareInfo;
pub use software::SoftwareList;
