pub struct InitialWindowSize {
    pub width: f64,
    pub height: f64,
    pub min_width: f64,
    pub min_height: f64,
}

pub fn initial_window_size(work_width: f64, work_height: f64) -> InitialWindowSize {
    let width = 960.0_f64.min((work_width - 32.0).max(1.0) * 0.86);
    let height = 640.0_f64.min((work_height - 48.0).max(1.0) * 0.86);
    InitialWindowSize {
        width,
        height,
        min_width: 600.0_f64.min(width),
        min_height: 420.0_f64.min(height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_window_and_minimum_fit_high_dpi_work_areas() {
        for (width, height, scale) in [
            (2560.0, 1520.0, 2.0),
            (1920.0, 1040.0, 1.5),
            (1280.0, 720.0, 2.0),
            (3840.0, 2080.0, 3.0),
        ] {
            let work_width = width / scale;
            let work_height = height / scale;
            let size = initial_window_size(work_width, work_height);
            assert!(size.width < work_width && size.height + 48.0 < work_height);
            assert!(size.min_width <= size.width && size.min_height <= size.height);
            assert!(size.width <= 960.0 && size.height <= 640.0);
        }
    }
}
