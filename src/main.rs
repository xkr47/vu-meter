use xcb;

fn main() {
    /*
    let segments: &[xcb::Segment] = &[
        xcb::Segment::new(100, 10, 140, 30),
        xcb::Segment::new(110, 25, 130, 60)
    ];
    let rectangles: &[xcb::Rectangle] = &[
        xcb::Rectangle::new(10, 50, 40, 20),
        xcb::Rectangle::new(80, 50, 10, 40)
    ];
*/
    let (conn, screen_num) = xcb::Connection::connect(None).unwrap();
    let screen = conn.get_setup().roots().nth(screen_num as usize).unwrap();

    let background = conn.generate_id();
    xcb::create_gc(&conn, background, screen.root(), &[
        (xcb::GC_FOREGROUND, screen.black_pixel()),
        (xcb::GC_GRAPHICS_EXPOSURES, 0),
    ]);
    let foreground = conn.generate_id();
    xcb::create_gc(&conn, foreground, screen.root(), &[
        (xcb::GC_FOREGROUND, screen.white_pixel()),
        (xcb::GC_GRAPHICS_EXPOSURES, 0),
    ]);

    let mut win_w: u16 = 150;
    let mut win_h: u16 = 150;

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
    conn.flush();

    let ch = [ 0.45, 0.1 ];
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

                        let e = ch.len() as i16;

                        let on_rect: Vec<xcb::Rectangle> = ch.iter().enumerate().map(
                            |(i, level)| rect(
                                ((e - i as i16 - 1) * x.0 + i as i16 * x.1) / e,
                                ((e - i as i16    ) * x.0 + (i as i16 + 1) * x.1) / e,
                                (y.0 as f32 * level + y.1 as f32 * (1f32 - level)) as i16,
                                y.1,
                            )
                        ).collect();
                        xcb::poly_fill_rectangle(&conn, win, foreground, &on_rect);

                        let off_rect: Vec<xcb::Rectangle> = ch.iter().enumerate().map(
                            |(i, level)| rect(
                                ((e - i as i16 - 1) * x.0 + i as i16 * x.1) / e,
                                ((e - i as i16    ) * x.0 + (i as i16 + 1) * x.1) / e,
                                y.0,
                                (y.0 as f32 * level + y.1 as f32 * (1f32 - level)) as i16,
                            )
                        ).collect();
                        xcb::poly_fill_rectangle(&conn, win, background, &off_rect);

                        /*
                        /* We draw the segements */
                        xcb::poly_segment(&conn, win, foreground, &segments);

                        /* We draw the rectangles */
                        xcb::poly_fill_rectangle(&conn, win, foreground, &rectangles);
                         */

                        /* We flush the request */
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
                        println!("Resize: {} x {}", win_w, win_h);
                        //break;
                    },
                    _ => {}
                }
            }
        }
    }
}

fn rect(x0: i16, x1: i16, y0: i16, y1: i16) -> xcb::Rectangle {
    assert!(x1 >= x0);
    assert!(y1 >= y0);
    xcb::Rectangle::new(x0, y0, (x1 - x0 + 1) as u16, (y1 - y0 + 1) as u16)
}
