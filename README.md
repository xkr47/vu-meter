# vu-meter
Audio [VU meter](https://en.wikipedia.org/wiki/VU_meter) for [JACK](https://jackaudio.org/) with any number of channels.

This is heavily inspired by the [cadence-jackmeter](https://github.com/falkTX/Cadence/blob/master/c%2B%2B/widgets/digitalpeakmeter.cpp) included in the [Cadence](https://github.com/falkTX/Cadence) tools. I rewrote it in [Rust](https://www.rust-lang.org/), with freely configurable amount of channels through commandline parameters. It uses [XCB](https://en.wikipedia.org/wiki/XCB) i.e. the X11 protocol for graphics.

```
USAGE:
    vu-meter [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --channels <NUM_CHANNELS>    Sets the number of input channels [default: 2]
```
