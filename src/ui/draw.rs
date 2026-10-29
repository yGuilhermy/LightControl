use windows::{
    core::w,
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::*,
            Gdi::HDC,
        },
    },
};

pub const BG: u32 = 0x101311;
pub const CARD: u32 = 0x191e1a;
pub const TILE: u32 = 0x242b25;
pub const TEXT: u32 = 0xf1f2e9;
pub const MUTED: u32 = 0x98a497;
pub const ACCENT: u32 = 0xd4e4b8;

pub fn color(c: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((c >> 16) & 255u32) as f32 / 255.,
        g: ((c >> 8) & 255u32) as f32 / 255.,
        b: (c & 255u32) as f32 / 255.,
        a: 1.,
    }
}
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
pub struct Renderer {
    pub target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    formats: Vec<IDWriteTextFormat>,
    halo: std::cell::RefCell<Option<(u32, ID2D1RadialGradientBrush)>>,
}
pub struct Graphics {
    factory: ID2D1Factory,
    _writer: IDWriteFactory,
    formats: Vec<IDWriteTextFormat>,
}
impl Graphics {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let writer: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut formats = Vec::new();
            for size in [11., 13., 16., 23., 54.] {
                let f = writer.CreateTextFormat(
                    w!("Segoe UI"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("pt-BR"),
                )?;
                f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                formats.push(f);
            }
            Ok(Self {
                factory,
                _writer: writer,
                formats,
            })
        }
    }
}
impl Renderer {
    pub fn new(graphics: &Graphics, dpi: f32) -> windows::core::Result<Self> {
        unsafe {
            let target = graphics
                .factory
                .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: dpi,
                    dpiY: dpi,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                })?;
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = target.CreateSolidColorBrush(&color(TEXT), None)?;
            Ok(Self {
                target,
                brush,
                formats: graphics.formats.clone(),
                halo: std::cell::RefCell::new(None),
            })
        }
    }
    pub fn begin(&self, dc: *mut std::ffi::c_void, width: i32, height: i32) -> bool {
        self.begin_at(dc, 0, 0, width, height)
    }
    pub fn begin_at(
        &self,
        dc: *mut std::ffi::c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> bool {
        unsafe {
            if self
                .target
                .BindDC(
                    HDC(dc),
                    &RECT {
                        left: x,
                        top: y,
                        right: x + width,
                        bottom: y + height,
                    },
                )
                .is_err()
            {
                return false;
            }
            self.target.BeginDraw();
            self.target.Clear(Some(&color(BG)));
        }
        true
    }
    pub fn end(&self) -> bool {
        unsafe { self.target.EndDraw(None, None).is_ok() }
    }
    pub fn text(&self, s: &str, x: f32, y: f32, width: f32, index: usize, c: u32) {
        unsafe {
            self.brush.SetColor(&color(c));
            let text: Vec<u16> = s.encode_utf16().collect();
            self.target.DrawText(
                &text,
                &self.formats[index],
                &rect(x, y, width, 80.),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
    pub fn centered(&self, s: &str, w: f32, h: f32, c: u32) {
        unsafe {
            let format = &self.formats[1];
            let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            self.brush.SetColor(&color(c));
            self.target.DrawText(
                &s.encode_utf16().collect::<Vec<_>>(),
                format,
                &rect(3., 0., w - 6., h),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
        }
    }
    pub fn round(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: u32) {
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(x, y, w, h),
                    radiusX: r,
                    radiusY: r,
                },
                &self.brush,
            );
        }
    }
    pub fn line(&self, x: f32, y: f32, x2: f32, y2: f32, c: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.DrawLine(
                D2D_POINT_2F { x, y },
                D2D_POINT_2F { x: x2, y: y2 },
                &self.brush,
                width,
                None,
            );
        }
    }
    pub fn ellipse(&self, x: f32, y: f32, rx: f32, ry: f32, c: u32) {
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.FillEllipse(
                &D2D1_ELLIPSE {
                    point: D2D_POINT_2F { x, y },
                    radiusX: rx,
                    radiusY: ry,
                },
                &self.brush,
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn gradient(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, colors: &[u32]) {
        unsafe {
            let stops: Vec<_> = colors
                .iter()
                .enumerate()
                .map(|(i, c)| D2D1_GRADIENT_STOP {
                    position: i as f32 / (colors.len() - 1).max(1) as f32,
                    color: color(*c),
                })
                .collect();
            if let Ok(stops) = self.target.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) {
                if let Ok(brush) = self.target.CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: D2D_POINT_2F { x, y },
                        endPoint: D2D_POINT_2F { x: x + w, y: y + h },
                    },
                    None,
                    &stops,
                ) {
                    self.target.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(x, y, w, h),
                            radiusX: radius,
                            radiusY: radius,
                        },
                        &brush,
                    );
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn stroke(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, c: u32) {
        unsafe {
            self.brush.SetColor(&color(c));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(x, y, w, h),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                1.,
                None,
            );
        }
    }
    pub fn bulb(&self, x: f32, y: f32, glow: f32, tint: u32, phase: f32) {
        let breath = 0.88 + phase.sin() * 0.12;
        let energy = glow * breath;
        unsafe {
            let mut cached = self.halo.borrow_mut();
            if cached.as_ref().is_none_or(|(c, _)| *c != tint) {
                let stops: Vec<_> = [(0., 0.21), (0.4, 0.12), (0.75, 0.025), (1., 0.)]
                    .into_iter()
                    .map(|(position, alpha)| {
                        let mut c = color(tint);
                        c.a = alpha;
                        D2D1_GRADIENT_STOP { position, color: c }
                    })
                    .collect();
                if let Ok(stops) = self.target.CreateGradientStopCollection(
                    &stops,
                    D2D1_GAMMA_2_2,
                    D2D1_EXTEND_MODE_CLAMP,
                ) {
                    if let Ok(brush) = self.target.CreateRadialGradientBrush(
                        &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                            center: D2D_POINT_2F { x, y: y - 3. },
                            gradientOriginOffset: D2D_POINT_2F { x: 0., y: 0. },
                            radiusX: 78.,
                            radiusY: 71.,
                        },
                        None,
                        &stops,
                    ) {
                        *cached = Some((tint, brush));
                    }
                }
            }
            if let Some((_, brush)) = cached.as_ref() {
                brush.SetCenter(D2D_POINT_2F { x, y: y - 3. });
                brush.SetOpacity(energy);
                self.target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: D2D_POINT_2F { x, y: y - 3. },
                        radiusX: 78.,
                        radiusY: 71.,
                    },
                    brush,
                );
            }
        }
        let glass = mix(0x414b40, tint, 0.28 + energy * 0.62);
        self.ellipse(x, y - 6., 29., 32., glass);
        self.round(x - 17., y + 9., 34., 25., 9., glass);
        self.ellipse(
            x - 6.,
            y - 16.,
            17.,
            15.,
            mix(glass, 0xffffff, 0.14 + energy * 0.2),
        );
        self.line(x - 17., y - 19., x - 13., y - 26., 0xf2f4df, 2.5);
        self.line(
            x - 8.,
            y + 19.,
            x - 4.,
            y - 1.,
            mix(glass, 0xffffff, 0.45),
            1.,
        );
        self.line(
            x + 8.,
            y + 19.,
            x + 4.,
            y - 1.,
            mix(glass, 0xffffff, 0.45),
            1.,
        );
        self.line(
            x - 4.,
            y - 1.,
            x + 4.,
            y - 1.,
            mix(glass, 0xffffff, 0.6),
            2.,
        );
        self.gradient(
            x - 15.,
            y + 30.,
            30.,
            9.,
            3.,
            &[0x7c8a77, 0x465044, 0x84927e],
        );
        self.round(x - 12., y + 40., 24., 4., 2., 0x6b7864);
        self.round(x - 7., y + 46., 14., 4., 2., 0x434e40);
        if energy > 0.05 {
            for i in 0..5 {
                let a = i as f32 * 1.2566 + phase * 0.22;
                self.ellipse(
                    x + a.cos() * 59.,
                    y - 6. + a.sin() * 51.,
                    1.4,
                    1.4,
                    mix(CARD, tint, energy * 0.45),
                );
            }
        }
    }
}

pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0., 1.);
    let channel = |shift: u32| {
        (((a >> shift) & 255u32) as f32 * (1. - t) + ((b >> shift) & 255u32) as f32 * t).round()
            as u32
    };
    channel(16) << 16 | channel(8) << 8 | channel(0)
}
