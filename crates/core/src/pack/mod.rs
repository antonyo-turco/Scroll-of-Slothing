//! Page packing algorithms. Coordinates are PDF points with a bottom-left origin
//! (like ReportLab), unless stated otherwise.

pub mod token_grid;

use serde::{Deserialize, Serialize};

/// One placed copy of an item on a page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// Index of the item in the caller's input list.
    pub item: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Result of a packing run. Every copy is either placed or listed in `skipped`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub page_w: f64,
    pub page_h: f64,
    pub pages: Vec<Vec<Placement>>,
    /// Item indices that could not fit on an empty page (one entry per item).
    pub skipped: Vec<usize>,
}
