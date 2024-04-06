use core::mem;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clap::Parser;
use jack::*;
use nix::sys::signalfd::signal::{SigHandler, signal, Signal};
use xcb::x;
use xcb::x::{PropMode, SendEventDest};

/// Jack VU-Meter inspired by cadence-jackmeter
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sets the number of input channels
    #[arg(short, long, default_value_t = 2)]
    channels: usize,

    /// Automatically connect ports to vu-meter on startup. Format is `channel:port[?]` where `channel` is the VU meter channel number starting from 1 and `port` is the output port to connect to. Can be given any number of times. Using a "?" suffix means connection is optional and will not fail startup.
    #[arg(short = 'C', long)]
    connect: Vec<String>,
}

fn main() {
    unsafe { signal(Signal::SIGHUP, SigHandler::SigIgn) }.unwrap();

    let args: Args = Args::parse();
    let num_channels = args.channels;

    let client = create_client().expect("Failed to create Jack client");
    let ports = setup_ports(&client, num_channels);
    let ports_unowned = ports.iter().map(|p| p.clone_unowned()).collect::<Vec<_>>();

    let process_handler_context = ProcessHandlerContext::new(
        ports,
    );

    let vu = process_handler_context.vu();

    let frame_dur_ms = 1000 * client.buffer_size() / client.sample_rate() as u32;

    let ac = match client.activate_async((), process_handler_context) {
        Ok(ac) => ac,
        Err(e) => {
            eprintln!("Failed to activate {:?}", e);
            return;
        }
    };

    if let Err(err) = connect_ports(&ac, args.connect, ports_unowned) {
        eprintln!("Failed to connect ports: {err:#?}");
        exit(1);
    }

    let (conn, screen_num) = match xcb::Connection::connect(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect to X server: {:?}", e);
            exit(1);
        }
    };
    let conn = Arc::new(conn);
    let screen = conn.get_setup().roots().nth(screen_num as usize).unwrap();

    let colormap = screen.default_colormap();

    let gc_bg = GcState::new(&conn, screen, colormap, 0x000000);
    let gc_meter_low = GcState::new(&conn, screen, colormap, 0x5DE73D);
    let gc_meter_med = GcState::new(&conn, screen, colormap, 0xFFFF00);
    let gc_meter_high = GcState::new(&conn, screen, colormap, 0xFF0000);
    let gc_grid_low = GcState::new(&conn, screen, colormap, 0x062806);
    let gc_grid_med1 = GcState::new(&conn, screen, colormap, 0x282806);
    let gc_grid_med2 = GcState::new(&conn, screen, colormap, 0x472806);
    let gc_grid_high = GcState::new(&conn, screen, colormap, 0x280F06);

    let mut win_w: u16 = 108;
    let mut win_h: u16 = 204;

    let title = "VU meter";

    let win: x::Window = conn.generate_id();
    conn.send_request(&x::CreateWindow {
        depth: x::COPY_FROM_PARENT as u8,
        wid: win,
        parent: screen.root(),
        x: 0,
        y: 0,
        width: win_w,
        height: win_h,
        border_width: 10,
        class: x::WindowClass::InputOutput,
        visual: screen.root_visual(),
        value_list: &[
            //x::Cw::BackPixel(screen.white_pixel()),
            x::Cw::EventMask(x::EventMask::EXPOSURE | x::EventMask::STRUCTURE_NOTIFY),
        ],
    });
    conn.send_request(&x::MapWindow {
        window: win,
    });
    conn.send_request(&x::ChangeProperty {
        mode: PropMode::Replace,
        window: win,
        property: x::ATOM_WM_NAME,
        r#type: x::ATOM_STRING,
        data: title.as_bytes(),
    });

    {
        // thread sending expose events at even intervals
        let conn = conn.clone();
        thread::spawn(move || {
            let refresh = Duration::from_millis(frame_dur_ms.max(50) as u64);
            loop {
                conn.send_request(&x::SendEvent {
                    propagate: true,
                    destination: SendEventDest::Window(win),
                    event_mask: x::EventMask::EXPOSURE,
                    event: &x::ExposeEvent::new(win, 0, 0, 0, 0, 0),
                });
                conn.flush().unwrap();
                thread::sleep(refresh);
            }
        });
    }

    let gc_bg = gc_bg.finalize();
    let gc_meter_low = gc_meter_low.finalize();
    let gc_meter_med = gc_meter_med.finalize();
    let gc_meter_high = gc_meter_high.finalize();
    let gc_grid_low = gc_grid_low.finalize();
    let gc_grid_med1 = gc_grid_med1.finalize();
    let gc_grid_med2 = gc_grid_med2.finalize();
    let gc_grid_high = gc_grid_high.finalize();

    conn.flush().unwrap();

    let mut prev_ch = vec![];
    let mut prev_locations = vec![];
    loop {
        let event = conn.wait_for_event();
        match event {
            Err(e) => { eprintln!("Error {e:?}"); break; }
            Ok(xcb::Event::X(r)) => {
                match r {
                    x::Event::Expose(event) => {
                        let is_fake_expose = event.width() == 0 && event.height() == 0;

                        let mut ch = vec![0f32; num_channels];
                        let ch = {
                            let mut src = vu.lock().unwrap();
                            mem::swap(&mut ch, &mut *src);
                            if is_fake_expose && ch == prev_ch {
                                continue;
                            }
                            ch
                        };
                        prev_ch = ch.clone();
                        /*
                        let evt_x0 = event.x();
                        let evt_y0 = event.y();
                        let evt_x1 = evt_x0 + event.width() - 1;
                        let evt_y1 = evt_y0 + event.height() - 1;
                        println!("Expose {},{} - {},{}", evt_x0, evt_y0, evt_x1, evt_y1);
                         */
                        let x: (i16, i16) = (0, win_w as i16 - 1);
                        let y: (i16, i16) = (0, win_h as i16 - 1);

                        // bar chart
                        let locations = ch.iter()
                            .enumerate()
                            .map(|(i, level)| {
                                let x0 = interp_i(x.0, x.1, i, num_channels);
                                let x1 = interp_i(x.0, x.1, i + 1, num_channels);
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

                        if is_fake_expose && locations == prev_locations {
                            continue;
                        }
                        prev_locations = locations.clone();

                        for (i, gc) in [gc_bg, gc_meter_high, gc_meter_med, gc_meter_low].iter().enumerate() {
                            let r: Vec<x::Rectangle> = locations.iter().flat_map(
                                |(x0, x1, y)|
                                    rect(*x0, *x1, y[i], y[i + 1] - 1)
                            ).collect();
                            if !r.is_empty() {
                                conn.send_request(&x::PolyFillRectangle {
                                    drawable: x::Drawable::Window(win),
                                    gc: *gc,
                                    rectangles: &r,
                                });
                            }
                        }

                        // grid
                        let y1 = interp_f(y.1, y.0, 0.25) as i16;
                        let y2 = interp_f(y.1, y.0, 0.5) as i16;
                        let y3 = interp_f(y.1, y.0, 0.7) as i16;
                        let y4 = interp_f(y.1, y.0, 0.83) as i16;
                        let y5 = interp_f(y.1, y.0, 0.9) as i16;
                        let y6 = interp_f(y.1, y.0, 0.96) as i16;
                        conn.send_request(&x::PolySegment {
                            drawable: x::Drawable::Window(win),
                            gc: gc_grid_low,
                            segments: &[
                                x::Segment { x1: x.0, y1,     x2: x.1, y2: y1 },
                                x::Segment { x1: x.0, y1: y2, x2: x.1, y2 },
                            ]
                        });
                        conn.send_request(&x::PolySegment {
                            drawable: x::Drawable::Window(win),
                            gc: gc_grid_med1,
                            segments: &[
                                x::Segment { x1: x.0, y1: y3, x2: x.1, y2: y3 },
                                x::Segment { x1: x.0, y1: y4, x2: x.1, y2: y4 },
                            ]
                        });
                        conn.send_request(&x::PolySegment {
                            drawable: x::Drawable::Window(win),
                            gc: gc_grid_med2,
                            segments: &[
                                x::Segment { x1: x.0, y1: y5, x2: x.1, y2: y5 },
                            ]
                        });
                        conn.send_request(&x::PolySegment {
                            drawable: x::Drawable::Window(win),
                            gc: gc_grid_high,
                            segments: &[
                                x::Segment { x1: x.0, y1: y6, x2: x.1, y2: y6 },
                            ]
                        });

                        conn.flush().unwrap();
                    }
                    x::Event::ConfigureNotify(event) => {
                        win_w = event.width();
                        win_h = event.height();
                        //println!("Resize: {} x {}", win_w, win_h);
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
        }
    }
}

struct GcState<'a, 'b> {
    cookie: x::AllocColorCookie,
    conn: &'a xcb::Connection,
    screen: &'b x::Screen,
}

impl<'a, 'b> GcState<'a, 'b> {
    fn new(conn: &'a xcb::Connection, screen: &'b x::Screen, colormap: x::Colormap, rgb: u32) -> GcState<'a, 'b> {
        let r = ((rgb >> 16) * 0x101) as u16;
        let g = (((rgb >> 8) & 0xFF) * 0x101) as u16;
        let b = ((rgb & 0xFF) * 0x101) as u16;
        let cookie = conn.send_request(&x::AllocColor { cmap: colormap, red: r, green: g, blue: b });
        GcState { cookie, conn, screen }
    }

    fn finalize(self) -> x::Gcontext {
        let pixel = self.conn.wait_for_reply(self.cookie).unwrap().pixel();
        let id = self.conn.generate_id();
        self.conn.send_request(&x::CreateGc {
            cid: id,
            drawable: x::Drawable::Window(self.screen.root()),
            value_list: &[
                x::Gc::Foreground(pixel),
                x::Gc::GraphicsExposures(false),
            ],
        });
        id
    }
}

fn rect(x0: i16, x1: i16, y0: i16, y1: i16) -> Option<x::Rectangle> {
    if x1 >= x0 && y1 >= y0 {
        Some(x::Rectangle { x: x0, y: y0, width: (x1 - x0 + 1) as u16, height: (y1 - y0 + 1) as u16 })
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

fn create_client() -> Result<Client, Error> {
    let options = ClientOptions::NO_START_SERVER /* | ClientOptions::USE_EXACT_NAME */;
    let (client, status) = Client::new("VU meter", options)?;
    if !(status & ClientStatus::NAME_NOT_UNIQUE).is_empty() {
        println!("We are not alone!");
    }
    Ok(client)
}

#[allow(clippy::default_constructed_unit_structs)]
fn setup_ports(client: &Client, num_channels: usize) -> Vec<Port<AudioIn>> {
    (0..num_channels).map(|chan|
        client.register_port(&format!("in_{}", chan + 1), AudioIn::default()).unwrap_or_else(|_| panic!("Failed to register port {}", chan))
    ).collect()
}

fn connect_ports<T, U>(ac: &AsyncClient<T, U>, ports: Vec<String>, dst_ports: Vec<Port<Unowned>>) -> Result<(), Error> {
    let client = ac.as_client();
    let num_channels = dst_ports.len();
    ports.iter()
        .map(|arg| {
            let optional = arg.ends_with('?');
            let arg = if optional { &arg[..arg.len()-1] } else { arg };
            let mut s = arg.splitn(2, ':');
            let num: usize = s.next().expect("Missing channel number").parse()
                .unwrap_or_else(|_| panic!("Malformed channel number, expected number in range 1–{}", num_channels));
            let port = s.next().expect("Missing port");
            if num < 1 || num > num_channels {
                panic!("Bad channel number, should be in range 1–{}", num_channels);
            }
            (num, port, optional)
        })
        .filter_map(|(dst_channel, src_port_name, optional)| {
            let src_port = match client.port_by_name(src_port_name) {
                Some(p) => p,
                None => {
                    eprintln!("No such port `{}` to connect to channel {}", src_port_name, dst_channel);
                    eprintln!("Available:");
                    for port in client.ports(None, Some(AudioOut.jack_port_type()), PortFlags::IS_OUTPUT) {
                        eprintln!("  - `{}`", port);
                    }
                    if optional {
                        return None
                    } else {
                        panic!("Bad port name");
                    }
                }
            };
            if !src_port.flags().contains(PortFlags::IS_OUTPUT) {
                panic!("Port `{}` is not an output port!", src_port_name);
            }
            let dst_port = &dst_ports[dst_channel - 1];
            let src_port_type = src_port.port_type().unwrap();
            let dst_port_type = dst_port.port_type().unwrap();
            if src_port_type != dst_port_type {
                panic!("Port `{n}` has wrong type — expected {e} but got {a}", n = src_port_name, e = dst_port_type, a = src_port_type);
            }
            Some((src_port_name, src_port, dst_channel, dst_port, optional))
        })
        .for_each(|(src_port_name, src_port, dst_channel, dst_port, optional)| {
            client.connect_ports(&src_port, dst_port)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to connect port `{}` to channel {}: {:#?}", src_port_name, dst_channel, e);
                    if !optional {
                        panic!("Bad connection");
                    }
                });
        });
    Ok(())
}

struct ProcessHandlerContext {
    ports: Vec<Port<AudioIn>>,
    vu: Arc<Mutex<Vec<f32>>>,
}

impl ProcessHandlerContext {
    fn new(
        ports: Vec<Port<AudioIn>>,
    ) -> ProcessHandlerContext {
        let num = ports.len();
        let mut vu = Vec::with_capacity(num);
        vu.resize(num, 0f32);
        ProcessHandlerContext {
            ports,
            vu: Arc::new(Mutex::new(vu)),
        }
    }

    fn vu(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.vu)
    }
}

impl ProcessHandler for ProcessHandlerContext {
    fn process(&mut self, _client: &Client, ps: &ProcessScope) -> Control {
        let mut vu = self.vu.lock().unwrap();
        self.ports.iter().enumerate().for_each(|(i, chan)| {
            let max_of_chan = chan.as_slice(ps).iter().map(|s| s.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
            vu[i] = vu[i].max(max_of_chan);
        });
        Control::Continue
    }
}
