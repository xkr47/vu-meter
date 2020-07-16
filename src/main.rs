use xcb;

fn main() {
    let (conn, screen_num) = xcb::Connection::connect(None).unwrap();
    let screen = conn.get_setup().roots().nth(screen_num as usize).unwrap();

    let colormap = screen.default_colormap();
    let mut gc = [
        0x000000u32, // background
        0x5DE73D, // meter low
        0xFFFF00, // meter med
        0xFF0000, // meter high
        0x062806, // grid low
        0x282806, // grid med 1
        0x472806, // grid med 2
        0x280F06, // grid high
    ].iter()
        .map(|rgb| [
            ((rgb >> 16) * 0x101) as u16,
            (((rgb >> 8) & 0xFF) * 0x101) as u16,
            ((rgb & 0xFF) * 0x101) as u16
        ])
        .map(|[r, g, b]| xcb::alloc_color(&conn, colormap, r, g, b))
        .collect::<Vec<xcb::AllocColorCookie>>()
        .into_iter()
        .map(|cookie| cookie.get_reply().unwrap().pixel())
        .map(|pixel| {
            let id = conn.generate_id();
            xcb::create_gc(&conn, id, screen.root(), &[
                (xcb::GC_FOREGROUND, pixel),
                (xcb::GC_GRAPHICS_EXPOSURES, 0),
            ]);
            id
        });
    let gc_bg = gc.next().unwrap();
    let gc_meter_low = gc.next().unwrap();
    let gc_meter_med = gc.next().unwrap();
    let gc_meter_high = gc.next().unwrap();
    let gc_grid_low = gc.next().unwrap();
    let gc_grid_med1 = gc.next().unwrap();
    let gc_grid_med2 = gc.next().unwrap();
    let gc_grid_high = gc.next().unwrap();
    assert!(gc.next().is_none());

    let mut win_w: u16 = 108;
    let mut win_h: u16 = 190;

    let title = "VU meter";

    let win = conn.generate_id();
    xcb::create_window(&conn,
                       xcb::COPY_FROM_PARENT as u8,
                       win,
                       screen.root(),
                       0, 0,
                       win_w, win_h,
                       10,
                       xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                       screen.root_visual(), &[
            //(xcb::CW_BACK_PIXEL, screen.black_pixel()),
            (xcb::CW_EVENT_MASK,
             xcb::EVENT_MASK_EXPOSURE |
                 xcb::EVENT_MASK_KEY_PRESS |
                 xcb::EVENT_MASK_STRUCTURE_NOTIFY
            ),
        ]
    );
    xcb::map_window(&conn, win);
    xcb::change_property(&conn, xcb::PROP_MODE_REPLACE as u8, win,
                         xcb::ATOM_WM_NAME, xcb::ATOM_STRING, 8, title.as_bytes());
    conn.flush();

    let ch = [0.00, 0.01, 0.45, 0.69, 0.71, 0.89, 0.91, 0.99, 1.00];
    loop {
        let event = conn.wait_for_event();
        match event {
            None => { break; }
            Some(event) => {
                let r = event.response_type() & !0x80;
                match r {
                    xcb::EXPOSE => {
                        /*
                        let event : &xcb::ExposeEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        let evt_x0 = event.x();
                        let evt_y0 = event.y();
                        let evt_x1 = evt_x0 + event.width() - 1;
                        let evt_y1 = evt_y0 + event.height() - 1;
                        println!("Expose {},{} - {},{}", evt_x0, evt_y0, evt_x1, evt_y1);
                         */
                        let x: (i16, i16) = (0, win_w as i16 - 1);
                        let y: (i16, i16) = (0, win_h as i16 - 1);

                        let e = ch.len();

                        // bar chart
                        let locations = ch.iter()
                            .enumerate()
                            .map(|(i, level)| {
                                let x0 = interp_i(x.0, x.1, i, e);
                                let x1 = interp_i(x.0, x.1, i + 1, e);
                                let yp = interp_f(y.1 + 1, y.0, *level) as i16;
                                let y = if *level < 0.7 {
                                    [y.0, yp, yp, yp, y.1 + 1]
                                } else {
                                    let ym1 = interp_f(y.1 + 1, y.0, 0.7) as i16;
                                    if *level < 0.9 {
                                        [y.0, yp, yp, ym1, y.1 + 1]
                                    } else {
                                        let ym2 = interp_f(y.1 + 1, y.0, 0.9) as i16;
                                        [y.0, yp, ym2, ym1, y.1 + 1]
                                    }
                                };
                                (x0, x1, y)
                            })
                            .collect::<Vec<(i16, i16, [i16; 5])>>();

                        for (i, gc) in [gc_bg, gc_meter_high, gc_meter_med, gc_meter_low].iter().enumerate() {
                            let r: Vec<xcb::Rectangle> = locations.iter().flat_map(
                                |(x0, x1, y)|
                                    rect(*x0, *x1, y[i], y[i+1]-1)
                            ).collect();
                            if !r.is_empty() {
                                xcb::poly_fill_rectangle(&conn, win, *gc, &r);
                            }
                        }

                        // grid
                        let y1 = interp_f(y.1, y.0, 0.25) as i16;
                        let y2 = interp_f(y.1, y.0, 0.5) as i16;
                        let y3 = interp_f(y.1, y.0, 0.7) as i16;
                        let y4 = interp_f(y.1, y.0, 0.83) as i16;
                        let y5 = interp_f(y.1, y.0, 0.9) as i16;
                        let y6 = interp_f(y.1, y.0, 0.96) as i16;
                        xcb::poly_segment(&conn, win, gc_grid_low, &[
                            xcb::Segment::new(x.0, y1, x.1, y1),
                            xcb::Segment::new(x.0, y2, x.1, y2),
                        ]);
                        xcb::poly_segment(&conn, win, gc_grid_med1, &[
                            xcb::Segment::new(x.0, y3, x.1, y3),
                            xcb::Segment::new(x.0, y4, x.1, y4),
                        ]);
                        xcb::poly_segment(&conn, win, gc_grid_med2, &[
                            xcb::Segment::new(x.0, y5, x.1, y5),
                        ]);
                        xcb::poly_segment(&conn, win, gc_grid_high, &[
                            xcb::Segment::new(x.0, y6, x.1, y6),
                        ]);

                        conn.flush();
                    },
                    xcb::KEY_PRESS => {
                        let event: &xcb::KeyPressEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        println!("Key '{}' pressed", event.detail());
                        //break;
                    },
                    xcb::CONFIGURE_NOTIFY => {
                        let event: &xcb::ConfigureNotifyEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        win_w = event.width();
                        win_h = event.height();
                        //println!("Resize: {} x {}", win_w, win_h);
                    },
                    _ => {}
                }
            }
        }
    }
}

fn rect(x0: i16, x1: i16, y0: i16, y1: i16) -> Option<xcb::Rectangle> {
    if x1 >= x0 && y1 >= y0 {
        Some(xcb::Rectangle::new(x0, y0, (x1 - x0 + 1) as u16, (y1 - y0 + 1) as u16))
    } else {
        None
    }
}

fn interp_i(a: i16, b: i16, pos: usize, max_pos: usize) -> i16 {
    (
        (
            a as i32 * (max_pos - pos) as i32
                +
                b as i32 * pos as i32
        ) / max_pos as i32
    ) as i16
}

fn interp_f(a: i16, b: i16, pos: f32) -> f32 {
    a as f32 * (1f32 - pos)
        +
        b as f32 * pos
}
