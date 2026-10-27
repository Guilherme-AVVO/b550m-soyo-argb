use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use soyo_argb::config::{self, Config, Scope};
use soyo_argb::device::{Device, hex_report};
use soyo_argb::protocol::{self, Direction, Mode, REPORT_LEN, Report, Rgb, ZONES, ZoneEffect};

#[derive(Parser)]
#[command(version, about = "Controle da iluminação ARGB da SOYO B550M (SONiX 0c45:7125)")]
struct Cli {
    /// Arquivo de configuração (padrão: $XDG_CONFIG_HOME/soyo-argb/config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Mostra o dispositivo e o estado gravado nele
    Status,
    /// Define o efeito de uma zona (1..3 = ARGB1..ARGB3) ou de todas (controle global), salva e aplica
    Set {
        /// 1, 2, 3 ou all
        #[arg(long)]
        zone: String,
        /// breathing|spectrum|static|wave|marquee|heartbeat|off
        #[arg(long)]
        mode: Mode,
        /// Cor 1 (rrggbb ou #rrggbb)
        #[arg(long)]
        color: Option<Rgb>,
        /// Cor 2 (respiração, letreiro, batimento)
        #[arg(long)]
        color2: Option<Rgb>,
        /// Cor aleatória (respiração, batimento)
        #[arg(long)]
        random: bool,
        /// Brilho 0..15
        #[arg(long)]
        brightness: Option<u8>,
        /// Velocidade 0..15 (15 = mais rápido)
        #[arg(long)]
        speed: Option<u8>,
        /// left|right (onda, letreiro)
        #[arg(long)]
        direction: Option<Direction>,
    },
    /// Troca as cores das zonas pelas de uma paleta (ex.: matugen), mantendo modo, brilho, velocidade e
    /// direção; salva e aplica só o que mudou
    Recolor {
        /// Nova cor 1 de toda zona cujo modo usa cor (rrggbb ou #rrggbb)
        #[arg(long)]
        color: Rgb,
        /// Nova cor 2 das zonas que já usam uma segunda cor
        #[arg(long)]
        color2: Option<Rgb>,
    },
    /// Define a quantidade de LEDs de um conector ARGB, salva e aplica
    LedCount {
        /// 1..3
        #[arg(long)]
        header: u8,
        /// 1..60
        #[arg(long)]
        count: u8,
    },
    /// Aplica a configuração inteira
    Apply,
    /// Volta ao perfil padrão do app original (estático azul, brilho 15, sem controle global) e aplica
    Restore,
    /// Reaplica a configuração quando o dispositivo reaparece ou o sistema volta da suspensão
    Daemon,
    /// DESENVOLVIMENTO: envia um Feature report cru de 64 bytes. Grava direto no dispositivo.
    Raw {
        /// 128 dígitos hexadecimais
        hex: String,
        /// Lê o Feature report seguinte e mostra
        #[arg(long)]
        read: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("erro: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let path = match cli.config {
        Some(p) => p,
        None => config::default_path()?,
    };
    match cli.cmd {
        Cmd::Status => status(),
        Cmd::Set { zone, mode, color, color2, random, brightness, speed, direction } => {
            let effect = ZoneEffect { mode, color, color2, random, brightness, speed, direction };
            effect.record()?;
            let mut cfg = config::load(&path)?;
            let zones: Vec<u8> = if zone == "all" {
                cfg.global = Some(effect);
                ZONES.collect()
            } else {
                let id: u8 = zone.parse().ok().filter(|z| ZONES.contains(z)).with_context(|| {
                    format!("zona {zone} inválida (use 1, 2, 3 ou all)")
                })?;
                cfg.global = None;
                cfg.set_zone(id, effect);
                vec![id]
            };
            config::save(&path, &cfg)?;
            config::apply(&cfg, &Device::open()?, Scope::Zones(&zones))
        }
        Cmd::Recolor { color, color2 } => {
            let mut cfg = config::load(&path)?;
            let before = cfg.clone();
            let zones = cfg.recolor(color, color2);
            if cfg == before {
                return Ok(());
            }
            config::save(&path, &cfg)?;
            if zones.is_empty() {
                return Ok(());
            }
            config::apply(&cfg, &Device::open()?, Scope::Zones(&zones))
        }
        Cmd::LedCount { header, count } => {
            protocol::led_count(header, count)?;
            let mut cfg = config::load(&path)?;
            cfg.led_count.set(header, Some(count));
            config::save(&path, &cfg)?;
            config::apply(&cfg, &Device::open()?, Scope::LedCount(header))
        }
        Cmd::Apply => config::apply(&config::load(&path)?, &Device::open()?, Scope::All),
        Cmd::Restore => {
            let cfg = Config::default();
            config::save(&path, &cfg)?;
            config::apply(&cfg, &Device::open()?, Scope::All)
        }
        Cmd::Daemon => soyo_argb::daemon::run(&path),
        Cmd::Raw { hex, read } => raw(&hex, read),
    }
}

fn status() -> Result<()> {
    let dev = Device::open()?;
    println!("dispositivo: {}", dev.path);
    let info = dev.read_all(&protocol::read_info())?;
    let id = &info[0][4..8];
    println!(
        "identificação (0x05): VID {:02x}{:02x} PID {:02x}{:02x}, bloco: {}",
        id[1],
        id[0],
        id[3],
        id[2],
        hex_report(&info[0][..12])
    );
    let global = protocol::parse_global(&dev.read_one(&protocol::read_global())?);
    println!("controle global: {}", if global { "ligado" } else { "desligado" });
    for z in ZONES {
        let rec = dev.read_one(&protocol::read_zone(z)?)?;
        let leds = protocol::parse_led_count(&dev.read_one(&protocol::read_led_count(z)?)?);
        let effect = match ZoneEffect::from_record(&rec) {
            Some(e) => describe(&e),
            None => format!("desconhecido ({})", hex_report(&rec[..8])),
        };
        println!("ARGB{z}: {effect}; LEDs: {leds}");
    }
    Ok(())
}

fn describe(e: &ZoneEffect) -> String {
    let mut parts = vec![e.mode.name().to_string()];
    if let Some(c) = e.color {
        parts.push(format!("cor {c}"));
    }
    if let Some(c) = e.color2 {
        parts.push(format!("cor2 {c}"));
    }
    if e.random {
        parts.push("aleatória".into());
    }
    if let Some(b) = e.brightness {
        parts.push(format!("brilho {b}"));
    }
    if let Some(s) = e.speed {
        parts.push(format!("velocidade {s}"));
    }
    if let Some(d) = e.direction {
        parts.push(format!("direção {}", if d == Direction::Left { "left" } else { "right" }));
    }
    parts.join(", ")
}

fn raw(hex: &str, read: bool) -> Result<()> {
    let hex: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if hex.len() != REPORT_LEN * 2 {
        bail!("esperados {} dígitos hexadecimais, recebidos {}", REPORT_LEN * 2, hex.len());
    }
    let mut r: Report = [0; REPORT_LEN];
    for (i, b) in r.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).context("hexadecimal inválido")?;
    }
    if let Some(resp) = Device::open()?.raw(&r, read)? {
        println!("{}", hex_report(&resp));
    }
    Ok(())
}
