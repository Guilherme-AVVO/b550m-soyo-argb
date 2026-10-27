//! Configuração persistente (`$XDG_CONFIG_HOME/soyo-argb/config.toml`) e aplicação no dispositivo.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::device::Device;
use crate::protocol::{self, Mode, Rgb, ZONES, ZoneEffect};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneConfig {
    pub id: u8,
    #[serde(flatten)]
    pub effect: ZoneEffect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedCount {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header1: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header2: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header3: Option<u8>,
}

impl LedCount {
    pub fn get(&self, header: u8) -> Option<u8> {
        match header {
            1 => self.header1,
            2 => self.header2,
            3 => self.header3,
            _ => None,
        }
    }

    pub fn set(&mut self, header: u8, count: Option<u8>) {
        match header {
            1 => self.header1 = count,
            2 => self.header2 = count,
            3 => self.header3 = count,
            _ => {}
        }
    }
}

/// Com `global` presente, o controle global fica ligado e todas as zonas recebem esse efeito;
/// sem ele, cada `[[zone]]` é gravada separadamente.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<ZoneEffect>,
    #[serde(default, rename = "zone")]
    pub zones: Vec<ZoneConfig>,
    #[serde(default)]
    pub led_count: LedCount,
}

/// Perfil padrão do app original (`Entry.xml`): estático azul, brilho 15.
pub fn default_effect() -> ZoneEffect {
    ZoneEffect { color: Some(Rgb(0, 0, 0xff)), brightness: Some(protocol::DEFAULT_BRIGHTNESS), ..ZoneEffect::new(Mode::Static) }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            global: None,
            zones: ZONES.map(|id| ZoneConfig { id, effect: default_effect() }).collect(),
            led_count: LedCount::default(),
        }
    }
}

impl Config {
    pub fn zone(&self, id: u8) -> Option<&ZoneEffect> {
        self.zones.iter().find(|z| z.id == id).map(|z| &z.effect)
    }

    pub fn set_zone(&mut self, id: u8, effect: ZoneEffect) {
        match self.zones.iter_mut().find(|z| z.id == id) {
            Some(z) => z.effect = effect,
            None => {
                self.zones.push(ZoneConfig { id, effect });
                self.zones.sort_by_key(|z| z.id);
            }
        }
    }

    /// Troca as cores que cada efeito já usa: `color` em todo modo com cor, `color2` só onde já estava
    /// definida. Modo, brilho, velocidade, direção e cor aleatória não mudam. Devolve as zonas que precisam
    /// ser regravadas no dispositivo: todas se o efeito global mudou, senão as zonas avulsas que mudaram.
    pub fn recolor(&mut self, color: Rgb, color2: Option<Rgb>) -> Vec<u8> {
        let changed: Vec<u8> = self
            .zones
            .iter_mut()
            .filter_map(|z| recolor_effect(&mut z.effect, color, color2).then_some(z.id))
            .collect();
        match self.global.as_mut().map(|g| recolor_effect(g, color, color2)) {
            Some(true) => ZONES.collect(),
            Some(false) => Vec::new(),
            None => changed,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(g) = &self.global {
            g.record().context("[global]")?;
        }
        let mut seen = Vec::new();
        for z in &self.zones {
            if !ZONES.contains(&z.id) {
                bail!("[[zone]] id {} inválido (use 1..=3)", z.id);
            }
            if seen.contains(&z.id) {
                bail!("[[zone]] id {} repetido", z.id);
            }
            seen.push(z.id);
            z.effect.record().with_context(|| format!("[[zone]] id {}", z.id))?;
        }
        for h in ZONES {
            if let Some(n) = self.led_count.get(h) {
                protocol::led_count(h, n).context("[led_count]")?;
            }
        }
        Ok(())
    }
}

fn recolor_effect(e: &mut ZoneEffect, color: Rgb, color2: Option<Rgb>) -> bool {
    let before = (e.color, e.color2);
    if e.color.is_some() {
        e.color = Some(color);
    }
    if let (Some(_), Some(c2)) = (e.color2, color2) {
        e.color2 = Some(c2);
    }
    (e.color, e.color2) != before
}

pub fn default_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME não definido")?).join(".config"),
    };
    Ok(base.join("soyo-argb").join("config.toml"))
}

/// Lê a configuração; arquivo ausente devolve o padrão.
pub fn load(path: &Path) -> Result<Config> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e).with_context(|| format!("falha ao ler {}", path.display())),
    };
    let cfg: Config = toml::from_str(&text).with_context(|| format!("{} inválido", path.display()))?;
    cfg.validate().with_context(|| format!("{} inválido", path.display()))?;
    Ok(cfg)
}

/// Grava de forma atômica (arquivo temporário + `rename`).
pub fn save(path: &Path, cfg: &Config) -> Result<()> {
    cfg.validate()?;
    let dir = path.parent().context("caminho de configuração sem diretório")?;
    fs::create_dir_all(dir).with_context(|| format!("falha ao criar {}", dir.display()))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, toml::to_string_pretty(cfg)?).with_context(|| format!("falha ao gravar {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("falha ao renomear para {}", path.display()))
}

/// O que enviar ao dispositivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope<'a> {
    /// Tudo: quantidades de LEDs e efeitos.
    All,
    /// Só os efeitos (global ou estas zonas).
    Zones(&'a [u8]),
    /// Só a quantidade de LEDs deste conector.
    LedCount(u8),
}

pub fn apply(cfg: &Config, dev: &Device, scope: Scope) -> Result<()> {
    cfg.validate()?;
    let headers: Vec<u8> = match scope {
        Scope::All => ZONES.collect(),
        Scope::LedCount(h) => vec![h],
        Scope::Zones(_) => Vec::new(),
    };
    for h in headers {
        if let Some(n) = cfg.led_count.get(h) {
            dev.write_plain(&protocol::led_count(h, n)?)?;
        }
    }
    let zones: Vec<u8> = match scope {
        Scope::All => ZONES.collect(),
        Scope::Zones(z) => z.to_vec(),
        Scope::LedCount(_) => return Ok(()),
    };
    let global_on = protocol::parse_global(&dev.read_one(&protocol::read_global())?);
    match &cfg.global {
        Some(effect) => {
            if !global_on {
                dev.write_plain(&protocol::global_control(true))?;
            }
            dev.write_effect(&protocol::all_zones_effect(effect)?)
        }
        None => {
            if global_on {
                dev.write_plain(&protocol::global_control(false))?;
            }
            for id in zones {
                if let Some(effect) = cfg.zone(id) {
                    dev.write_effect(&protocol::zone_effect(id, effect)?)?;
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: Rgb = Rgb(0xc2, 0xc6, 0xd5);
    const PALETTE2: Rgb = Rgb(0xd4, 0xc4, 0xac);

    fn effect(mode: Mode, color: Option<Rgb>, color2: Option<Rgb>) -> ZoneEffect {
        ZoneEffect { color, color2, speed: Some(4), brightness: Some(9), ..ZoneEffect::new(mode) }
    }

    #[test]
    fn recolor_keeps_effect_and_only_fills_colors_in_use() {
        let red = Some(Rgb(0xff, 0, 0));
        let mut cfg = Config {
            global: None,
            zones: vec![
                ZoneConfig { id: 1, effect: effect(Mode::Breathing, red, None) },
                ZoneConfig { id: 2, effect: effect(Mode::Marquee, red, Some(Rgb(0, 0xff, 0))) },
                ZoneConfig { id: 3, effect: effect(Mode::Spectrum, None, None) },
            ],
            led_count: LedCount::default(),
        };
        assert_eq!(cfg.recolor(PALETTE, Some(PALETTE2)), vec![1, 2]);
        assert_eq!(cfg.zone(1), Some(&effect(Mode::Breathing, Some(PALETTE), None)));
        assert_eq!(cfg.zone(2), Some(&effect(Mode::Marquee, Some(PALETTE), Some(PALETTE2))));
        assert_eq!(cfg.zone(3), Some(&effect(Mode::Spectrum, None, None)));
        // Mesma paleta de novo: nada a regravar na EEPROM.
        assert!(cfg.recolor(PALETTE, Some(PALETTE2)).is_empty());
    }

    #[test]
    fn recolor_with_global_rewrites_every_zone_only_when_global_changes() {
        let mut cfg = Config { global: Some(default_effect()), ..Config::default() };
        assert_eq!(cfg.recolor(PALETTE, None), ZONES.collect::<Vec<_>>());
        cfg.global = Some(ZoneEffect::new(Mode::Wave));
        assert!(cfg.recolor(Rgb(1, 2, 3), None).is_empty());
        // As zonas avulsas guardadas também seguem a paleta, para quando o controle global for desligado.
        assert_eq!(cfg.zone(1).and_then(|z| z.color), Some(Rgb(1, 2, 3)));
    }
}
