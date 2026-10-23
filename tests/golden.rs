//! Compara a codificação com os Feature reports capturados do app original (`re/fixtures/`).

use soyo_argb::protocol::{
    self, BlockWrite, Direction, Mode, REPORT_LEN, Report, Rgb, ZoneEffect,
};

fn parse_hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn fixture(name: &str) -> Vec<Report> {
    let path = format!("{}/re/fixtures/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let hex = l.strip_prefix("FEATURE ").unwrap_or_else(|| panic!("{path}: linha sem FEATURE: {l}"));
            parse_hex(hex).try_into().unwrap()
        })
        .collect()
}

fn assert_fixture(name: &str, w: &BlockWrite) {
    assert_eq!(fixture(name), w.sets().to_vec(), "{name}");
}

fn rgb(s: &str) -> Rgb {
    s.parse().unwrap()
}

fn effect(mode: Mode) -> ZoneEffect {
    ZoneEffect::new(mode)
}

fn static_color(color: &str, brightness: u8) -> ZoneEffect {
    ZoneEffect { color: Some(rgb(color)), brightness: Some(brightness), ..effect(Mode::Static) }
}

#[test]
fn static_colors_and_zones() {
    let red = static_color("ff0000", 15);
    assert_fixture("03-static-red-g3", &protocol::zone_effect(3, &red).unwrap());
    assert_fixture("04-static-green-g3", &protocol::zone_effect(3, &static_color("00ff00", 15)).unwrap());
    assert_fixture("05-static-blue-g3", &protocol::zone_effect(3, &static_color("0000ff", 15)).unwrap());
    assert_fixture("06-static-red-g2", &protocol::zone_effect(2, &red).unwrap());
    assert_fixture("06b-static-red-g1", &protocol::zone_effect(1, &red).unwrap());
    assert_fixture("07-static-red-g3-bright60", &protocol::zone_effect(3, &static_color("ff0000", 2)).unwrap());
}

#[test]
fn breathing() {
    let base = ZoneEffect {
        color: Some(rgb("ff0000")),
        brightness: Some(15),
        speed: Some(0),
        ..effect(Mode::Breathing)
    };
    assert_fixture("08-breathing-slow-g3", &protocol::zone_effect(3, &base).unwrap());
    let fast = ZoneEffect { speed: Some(15), ..base.clone() };
    assert_fixture("09-breathing-fast-g3", &protocol::zone_effect(3, &fast).unwrap());
    let two = ZoneEffect { color2: Some(rgb("00ff00")), ..fast.clone() };
    assert_fixture("10-breathing-color2-g3", &protocol::zone_effect(3, &two).unwrap());
    let random = ZoneEffect { random: true, ..two };
    assert_fixture("11-breathing-random-g3", &protocol::zone_effect(3, &random).unwrap());
}

#[test]
fn other_modes() {
    let spectrum = ZoneEffect { brightness: Some(5), speed: Some(5), ..effect(Mode::Spectrum) };
    assert_fixture("12-spectrum-g3", &protocol::zone_effect(3, &spectrum).unwrap());
    let wave = |d| ZoneEffect { brightness: Some(5), speed: Some(5), direction: Some(d), ..effect(Mode::Wave) };
    assert_fixture("13-wave-left-g3", &protocol::zone_effect(3, &wave(Direction::Left)).unwrap());
    assert_fixture("14-wave-right-g3", &protocol::zone_effect(3, &wave(Direction::Right)).unwrap());
    let marquee = ZoneEffect {
        color: Some(rgb("000000")),
        brightness: Some(5),
        speed: Some(5),
        direction: Some(Direction::Left),
        ..effect(Mode::Marquee)
    };
    assert_fixture("15-marquee-g3", &protocol::zone_effect(3, &marquee).unwrap());
    let heartbeat = ZoneEffect {
        color: Some(rgb("00ff00")),
        color2: Some(rgb("0000ff")),
        brightness: Some(5),
        speed: Some(5),
        ..effect(Mode::Heartbeat)
    };
    assert_fixture("16-heartbeat-g3", &protocol::zone_effect(3, &heartbeat).unwrap());
    assert_fixture("17-off-g3", &protocol::zone_effect(3, &effect(Mode::Off)).unwrap());
}

#[test]
fn led_count_and_global() {
    assert_fixture("19-ledcount-h3-40", &protocol::led_count(3, 40).unwrap());
    assert_fixture("22-global-on", &protocol::global_control(true));
    assert_fixture("23-global-off", &protocol::global_control(false));
    assert_fixture("22b-write-all-off", &protocol::all_zones_effect(&effect(Mode::Off)).unwrap());
    assert_fixture("22c-write-all-static-red", &protocol::all_zones_effect(&static_color("ff0000", 5)).unwrap());
}

#[test]
fn checksum_matches_device_sum_for_fixtures() {
    // O último report de cada fixture é `04 02`; o dispositivo devolveu a soma do bloco (ver docs/protocol.md §2.2).
    let w = protocol::zone_effect(3, &static_color("ff0000", 15)).unwrap();
    assert_eq!(w.expected_sum(), 0x0327);
    assert_eq!(protocol::led_count(3, 40).unwrap().expected_sum(), 0x01fd);
}

#[test]
fn verify_challenge_matches_device() {
    let path = format!("{}/re/fixtures/verify-challenge.txt", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    let mut n = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let (r, v) = line.split_once(' ').unwrap();
        let r: [u8; REPORT_LEN] = parse_hex(r).try_into().unwrap();
        assert_eq!(protocol::verify_response(&r), u32::from_str_radix(v, 16).unwrap());
        n += 1;
    }
    assert!(n >= 5);
}

#[test]
fn record_roundtrip() {
    let e = ZoneEffect {
        color: Some(rgb("123456")),
        color2: Some(rgb("abcdef")),
        brightness: Some(9),
        speed: Some(3),
        direction: Some(Direction::Right),
        ..effect(Mode::Marquee)
    };
    assert_eq!(ZoneEffect::from_record(&e.record().unwrap()), Some(e));
}

#[test]
fn rejects_invalid_input() {
    let red = static_color("ff0000", 15);
    assert!(protocol::zone_effect(0, &red).is_err());
    assert!(protocol::zone_effect(4, &red).is_err());
    assert!(protocol::led_count(4, 30).is_err());
    assert!(protocol::led_count(1, 0).is_err());
    assert!(protocol::led_count(1, 61).is_err());
    assert!(static_color("ff0000", 16).record().is_err());
    let static_with_speed = ZoneEffect { speed: Some(3), ..red.clone() };
    assert!(static_with_speed.record().is_err());
    let spectrum_with_color = ZoneEffect { color: Some(rgb("ff0000")), ..effect(Mode::Spectrum) };
    assert!(spectrum_with_color.record().is_err());
    let wave_random = ZoneEffect { random: true, ..effect(Mode::Wave) };
    assert!(wave_random.record().is_err());
    assert!(effect(Mode::Breathing).record().is_err(), "respiração sem cor");
    assert!("12345".parse::<Rgb>().is_err());
}
