//! DaedalusEngine simulation core.
//!
//! Power-electronics circuit simulator: modified nodal analysis with trapezoidal integration and
//! backward-Euler damping after switching events, piecewise-linear switches with switching-instant
//! interpolation, SPICE-grade semiconductor models solved by Newton iteration, local-truncation-error
//! step refinement, and a control-block scheduler (continuous, discrete and logic blocks).

pub mod accuracy;
pub mod doc;
pub mod expr;
pub mod library;
pub mod models;
pub mod montecarlo;
pub mod netlist;
pub mod runner;
pub mod sim;
pub mod sparse;
pub mod sweep;
pub mod util;

pub const VERSION: &str = "V1.4.1";
