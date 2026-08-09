fn main() {
    for scale in [1.0f32, 2.0] {
        let (w, h, data) = gpui_sfsymbols::SfSymbol::new("speedometer")
            .size(13.0)
            .scale(scale)
            .symbol_scale(gpui_sfsymbols::SymbolScale::Medium)
            .rendering_mode(gpui_sfsymbols::RenderingMode::Monochrome)
            .color(0x434343)
            .render_rgba()
            .expect("render failed");
        let (mut x0, mut x1, mut y0, mut y1) = (w, 0, h, 0);
        for y in 0..h {
            for x in 0..w {
                let a = data[((y * w + x) * 4 + 3) as usize];
                if a > 60 {
                    x0 = x0.min(x); x1 = x1.max(x);
                    y0 = y0.min(y); y1 = y1.max(y);
                }
            }
        }
        println!("scale {scale}: canvas {w}x{h}, ink bbox {}x{} (x{x0}..{x1} y{y0}..{y1})", x1 - x0 + 1, y1 - y0 + 1);
    }
}
