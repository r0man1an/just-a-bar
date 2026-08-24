#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarGeometry {
    pub height: f64,
    pub font_pt: f64,
    pub section_spacing: i32,
    pub workspace_gap: i32,
    pub badge_width: i32,
    pub badge_height: i32,
    pub badge_radius: f64,
    pub applet_padding: f64,
}

impl BarGeometry {
    pub fn compute(bar_height: u32) -> Self {
        let height = bar_height.max(1) as f64;

        BarGeometry {
            height,
            font_pt: (height / 3.0).max(8.0),
            section_spacing: (height * 0.85).round().max(1.0) as i32,
            workspace_gap: (height * 0.30).round().max(1.0) as i32,
            badge_width: (height * 0.85).round().max(1.0) as i32,
            badge_height: (height * 0.67).round().max(1.0) as i32,
            badge_radius: (height * 0.15).round().max(4.0),
            applet_padding: (height * 0.12).round().max(2.0),
        }
    }
}
