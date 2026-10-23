//! Codificação do protocolo do controlador SONiX 0c45:7125. Sem I/O.
//!
//! Cada byte aqui vem de `docs/protocol.md`; não acrescente comandos que não estejam lá.

use std::fmt;
use std::str::FromStr;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const VID: u16 = 0x0C45;
pub const PID: u16 = 0x7125;
/// Tamanho de todo Feature report, sem o byte de Report ID.
pub const REPORT_LEN: usize = 64;
/// Espera entre SET e GET e entre comandos (`CommandDelayTime` do `Entry.xml`).
pub const COMMAND_DELAY_MS: u64 = 10;
/// Espera após o dispositivo aparecer (`DeviceArrivalDelayTime`).
pub const ARRIVAL_DELAY_MS: u64 = 1000;
/// Tentativas de um comando com ack, como no app original.
pub const COMMAND_RETRIES: usize = 5;

pub const MAX_LEVEL: u8 = 15;
pub const BRIGHTNESS_LEVELS: [u8; 16] = [
    0, 45, 60, 75, 90, 105, 120, 135, 150, 165, 180, 195, 210, 225, 240, 255,
];
pub const SPEED_LEVELS: [u8; 16] = [38, 36, 34, 32, 30, 28, 26, 24, 22, 20, 18, 16, 14, 12, 10, 8];
pub const DEFAULT_BRIGHTNESS: u8 = 15;
pub const DEFAULT_SPEED: u8 = 11;
/// Limites do campo de quantidade de LEDs na UI original (`LightingAdvUI.xml`).
pub const LED_COUNT_MIN: u8 = 1;
pub const LED_COUNT_MAX: u8 = 60;
/// Zonas ARGB implementadas (ARGB1..ARGB3).
pub const ZONES: std::ops::RangeInclusive<u8> = 1..=3;

pub type Report = [u8; REPORT_LEN];

const OP_SUM: u8 = 0x02;
const OP_INFO: u8 = 0x05;
const OP_VERIFY: u8 = 0xab;
const OP_WRITE_ALL: u8 = 0xb1;
const OP_GLOBAL_WRITE: u8 = 0xb2;
const OP_GLOBAL_READ: u8 = 0xb3;
const OP_WRITE_ZONE: [u8; 3] = [0x81, 0x83, 0x85];
const OP_READ_ZONE: [u8; 3] = [0x80, 0x82, 0x84];
const OP_LED_WRITE: [u8; 3] = [0xd1, 0xd3, 0xd5];
const OP_LED_READ: [u8; 3] = [0xd2, 0xd4, 0xd6];

const MARKER: [u8; 2] = [0xaa, 0x55];
/// Cor que o app grava no modo Onda (a UI não expõe cor para esse modo).
const WAVE_COLOR: Rgb = Rgb(0xff, 0xff, 0xff);
/// Velocidade que o app grava no modo Estático (nível 0).
const STATIC_SPEED_LEVEL: u8 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Breathing,
    Spectrum,
    Static,
    Wave,
    Marquee,
    Heartbeat,
    Off,
}

impl Mode {
    pub const ALL: [Mode; 7] = [
        Mode::Static,
        Mode::Breathing,
        Mode::Spectrum,
        Mode::Wave,
        Mode::Marquee,
        Mode::Heartbeat,
        Mode::Off,
    ];

    fn wire(self) -> u8 {
        match self {
            Mode::Off => 0x00,
            Mode::Breathing => 0x01,
            Mode::Spectrum => 0x03,
            Mode::Static => 0x04,
            Mode::Wave => 0x05,
            Mode::Heartbeat => 0x07,
            Mode::Marquee => 0x10,
        }
    }

    fn from_wire(b: u8) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.wire() == b)
    }

    /// Quais parâmetros o modo aceita (matriz de `docs/protocol.md` §5).
    pub fn params(self) -> Params {
        let none = Params::default();
        match self {
            Mode::Static => Params { color1: true, brightness: true, ..none },
            Mode::Breathing | Mode::Heartbeat => Params {
                color1: true,
                color2: true,
                random: true,
                speed: true,
                brightness: true,
                ..none
            },
            Mode::Spectrum => Params { speed: true, brightness: true, ..none },
            Mode::Wave => Params { speed: true, brightness: true, direction: true, ..none },
            Mode::Marquee => Params {
                color1: true,
                color2: true,
                speed: true,
                brightness: true,
                direction: true,
                ..none
            },
            Mode::Off => none,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Breathing => "breathing",
            Mode::Spectrum => "spectrum",
            Mode::Static => "static",
            Mode::Wave => "wave",
            Mode::Marquee => "marquee",
            Mode::Heartbeat => "heartbeat",
            Mode::Off => "off",
        }
    }
}

impl FromStr for Mode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Mode::ALL
            .into_iter()
            .find(|m| m.name() == s)
            .ok_or_else(|| anyhow::anyhow!("modo desconhecido: {s} (use breathing|spectrum|static|wave|marquee|heartbeat|off)"))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Params {
    pub color1: bool,
    pub color2: bool,
    pub random: bool,
    pub speed: bool,
    pub brightness: bool,
    pub direction: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Left,
    Right,
}

impl Direction {
    fn wire(self) -> u8 {
        match self {
            Direction::Left => 0,
            Direction::Right => 1,
        }
    }
}

impl FromStr for Direction {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "left" => Ok(Direction::Left),
            "right" => Ok(Direction::Right),
            _ => bail!("direção desconhecida: {s} (use left|right)"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgb(pub u8, pub u8, pub u8);

impl FromStr for Rgb {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("cor inválida: {s} (use rrggbb ou #rrggbb)");
        }
        let v = u32::from_str_radix(hex, 16)?;
        Ok(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

impl TryFrom<String> for Rgb {
    type Error = anyhow::Error;
    fn try_from(s: String) -> Result<Self> {
        s.parse()
    }
}

impl From<Rgb> for String {
    fn from(c: Rgb) -> String {
        c.to_string()
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// Efeito de uma zona. Campos `None` usam o padrão do modo; campos que o modo não aceita devem ser `None`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneEffect {
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color2: Option<Rgb>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub random: bool,
    /// Nível 0..=15.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    /// Nível 0..=15 (15 = mais rápido).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
}

impl ZoneEffect {
    pub fn new(mode: Mode) -> Self {
        ZoneEffect {
            mode,
            color: None,
            color2: None,
            random: false,
            brightness: None,
            speed: None,
            direction: None,
        }
    }

    fn validate(&self) -> Result<()> {
        let p = self.mode.params();
        let mode = self.mode.name();
        let check = |given: bool, allowed: bool, what: &str| -> Result<()> {
            if given && !allowed {
                bail!("o modo {mode} não aceita {what}");
            }
            Ok(())
        };
        check(self.color.is_some(), p.color1, "cor")?;
        check(self.color2.is_some(), p.color2, "cor 2")?;
        check(self.random, p.random, "cor aleatória")?;
        check(self.brightness.is_some(), p.brightness, "brilho")?;
        check(self.speed.is_some(), p.speed, "velocidade")?;
        check(self.direction.is_some(), p.direction, "direção")?;
        if p.color1 && self.color.is_none() {
            bail!("o modo {mode} exige uma cor");
        }
        for (name, level) in [("brilho", self.brightness), ("velocidade", self.speed)] {
            if let Some(l) = level
                && l > MAX_LEVEL
            {
                bail!("{name} {l} fora do intervalo 0..={MAX_LEVEL}");
            }
        }
        Ok(())
    }

    /// Registro de iluminação de 64 bytes (`docs/protocol.md` §5).
    pub fn record(&self) -> Result<Report> {
        self.validate()?;
        let mut r = [0u8; REPORT_LEN];
        if self.mode == Mode::Off {
            return Ok(r);
        }
        let p = self.mode.params();
        r[0x00] = self.mode.wire();
        let color1 = match self.mode {
            Mode::Wave => Some(WAVE_COLOR),
            _ => self.color,
        };
        if let Some(c) = color1 {
            r[0x01..0x04].copy_from_slice(&[c.0, c.1, c.2]);
        }
        if p.color2 {
            let c = self.color2.or(self.color).expect("validate exige cor");
            r[0x05..0x08].copy_from_slice(&[c.0, c.1, c.2]);
        }
        r[0x39] = u8::from(self.random);
        r[0x3a] = BRIGHTNESS_LEVELS[usize::from(self.brightness.unwrap_or(DEFAULT_BRIGHTNESS))];
        let speed = match self.mode {
            Mode::Static => STATIC_SPEED_LEVEL,
            _ => self.speed.unwrap_or(DEFAULT_SPEED),
        };
        r[0x3b] = SPEED_LEVELS[usize::from(speed)];
        r[0x3c] = self.direction.unwrap_or_default().wire();
        r[0x3e..0x40].copy_from_slice(&MARKER);
        Ok(r)
    }

    /// Lê um registro vindo do dispositivo. `None` se o modo ou os níveis não forem conhecidos.
    pub fn from_record(r: &Report) -> Option<ZoneEffect> {
        let mode = Mode::from_wire(r[0])?;
        let mut e = ZoneEffect::new(mode);
        if mode == Mode::Off {
            return Some(e);
        }
        let p = mode.params();
        let rgb = |o: usize| Rgb(r[o], r[o + 1], r[o + 2]);
        let level = |table: &[u8; 16], v: u8| table.iter().position(|&x| x == v).map(|i| i as u8);
        if p.color1 {
            e.color = Some(rgb(0x01));
        }
        if p.color2 && rgb(0x05) != rgb(0x01) {
            e.color2 = Some(rgb(0x05));
        }
        e.random = p.random && r[0x39] == 1;
        e.brightness = Some(level(&BRIGHTNESS_LEVELS, r[0x3a])?);
        if p.speed {
            e.speed = Some(level(&SPEED_LEVELS, r[0x3b])?);
        }
        if p.direction {
            e.direction = Some(if r[0x3c] == 0 { Direction::Left } else { Direction::Right });
        }
        Some(e)
    }
}

/// Escrita de um bloco: cabeçalho com ack, o bloco de 64 B e a soma de verificação (`docs/protocol.md` §2.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockWrite {
    pub header: Report,
    pub block: Report,
}

impl BlockWrite {
    fn new(op: u8, block: Report) -> Self {
        BlockWrite { header: command(op, 0, 1), block }
    }

    /// Os três Feature reports que o host envia, na ordem.
    pub fn sets(&self) -> [Report; 3] {
        [self.header, self.block, sum_command()]
    }

    pub fn expected_sum(&self) -> u32 {
        checksum(&self.block)
    }
}

/// Leitura de `blocks` blocos de 64 B (`docs/protocol.md` §2.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRead {
    pub header: Report,
    pub blocks: usize,
}

impl BlockRead {
    fn new(op: u8, blocks: u8) -> Self {
        BlockRead { header: command(op, 0, blocks), blocks: usize::from(blocks) }
    }
}

/// `[04 op p2 00 00 00 00 00 p8]`.
pub fn command(op: u8, p2: u8, p8: u8) -> Report {
    let mut r = [0u8; REPORT_LEN];
    r[0] = 0x04;
    r[1] = op;
    r[2] = p2;
    r[8] = p8;
    r
}

pub fn sum_command() -> Report {
    command(OP_SUM, 0, 0)
}

pub fn verify_command() -> Report {
    command(OP_VERIFY, 0, 0)
}

/// Resposta esperada do dispositivo ao desafio `r` (`docs/protocol.md` §2.3).
pub fn verify_response(r: &Report) -> u32 {
    let b0 = r[0x0c].wrapping_add(r[0x03]);
    let b1 = r[0x3f].wrapping_mul(8);
    let b2 = r[0x06].wrapping_sub(r[0x0d]);
    let b3 = r[0x24] & 0xa5;
    u32::from_be_bytes([b0, b1, b2, b3]) ^ 0x5471_1688
}

/// Soma de todos os bytes, como o dispositivo devolve em `04 02`.
pub fn checksum(block: &[u8]) -> u32 {
    block.iter().map(|&b| u32::from(b)).sum()
}

/// Valor `u32 LE` que vem em `resp[4..8]` da soma.
pub fn sum_value(resp: &Report) -> u32 {
    u32::from_le_bytes([resp[4], resp[5], resp[6], resp[7]])
}

/// Ack de um comando: `resp[3] == 0x01`.
pub fn is_ack(resp: &Report) -> bool {
    resp[3] == 0x01
}

fn zone_index(zone: u8) -> Result<usize> {
    if !ZONES.contains(&zone) {
        bail!("zona {zone} inválida (use 1..=3: ARGB1..ARGB3)");
    }
    Ok(usize::from(zone - 1))
}

/// Grava o efeito numa zona ARGB (1..=3).
pub fn zone_effect(zone: u8, e: &ZoneEffect) -> Result<BlockWrite> {
    Ok(BlockWrite::new(OP_WRITE_ZONE[zone_index(zone)?], e.record()?))
}

/// Grava o efeito em todas as zonas (registro global, usado com o controle global ligado).
pub fn all_zones_effect(e: &ZoneEffect) -> Result<BlockWrite> {
    Ok(BlockWrite::new(OP_WRITE_ALL, e.record()?))
}

/// Liga/desliga o controle global.
pub fn global_control(enabled: bool) -> BlockWrite {
    let mut block = [0u8; REPORT_LEN];
    block[0] = u8::from(enabled);
    block[1] = OP_GLOBAL_WRITE;
    block[8] = 0x01;
    BlockWrite::new(OP_GLOBAL_WRITE, block)
}

/// Quantidade de LEDs do conector ARGB `header` (1..=3).
pub fn led_count(header: u8, count: u8) -> Result<BlockWrite> {
    let i = zone_index(header).map_err(|_| anyhow::anyhow!("header {header} inválido (use 1..=3)"))?;
    if !(LED_COUNT_MIN..=LED_COUNT_MAX).contains(&count) {
        bail!("quantidade de LEDs {count} fora do intervalo {LED_COUNT_MIN}..={LED_COUNT_MAX}");
    }
    let op = OP_LED_WRITE[i];
    let mut block = [0u8; REPORT_LEN];
    block[0] = count;
    block[1] = op;
    block[8] = 0x01;
    block[0x3e..0x40].copy_from_slice(&MARKER);
    Ok(BlockWrite::new(op, block))
}

pub fn read_info() -> BlockRead {
    BlockRead::new(OP_INFO, 2)
}

pub fn read_global() -> BlockRead {
    BlockRead::new(OP_GLOBAL_READ, 1)
}

pub fn read_zone(zone: u8) -> Result<BlockRead> {
    Ok(BlockRead::new(OP_READ_ZONE[zone_index(zone)?], 1))
}

pub fn read_led_count(header: u8) -> Result<BlockRead> {
    Ok(BlockRead::new(OP_LED_READ[zone_index(header)?], 1))
}

/// Estado do controle global a partir do bloco lido por [`read_global`].
pub fn parse_global(block: &Report) -> bool {
    block[0] == 1
}

/// Quantidade de LEDs a partir do bloco lido por [`read_led_count`].
pub fn parse_led_count(block: &Report) -> u8 {
    block[0]
}
