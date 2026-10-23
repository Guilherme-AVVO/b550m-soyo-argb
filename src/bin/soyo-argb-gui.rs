//! Janela do `soyo-argb`: escolhe a zona, o efeito e aplica na placa.
//!
//! Layout absoluto, pintado à mão: barra lateral com as zonas, card de destaque com a prévia
//! animada do efeito, card de cores com o botão de aplicar e uma fileira de ajustes.

use std::f32::consts::TAU;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Mesh, Pos2, Rect,
    Response, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, epaint::Shadow, pos2, vec2,
};
use hidapi::HidApi;

use soyo_argb::config::{self, Config, Scope};
use soyo_argb::device::{self, Device};
use soyo_argb::protocol::{
    DEFAULT_BRIGHTNESS, DEFAULT_SPEED, Direction, LED_COUNT_MAX, LED_COUNT_MIN, MAX_LEVEL, Mode, Rgb, ZONES,
    ZoneEffect,
};

// Paleta: grafite azulado, como a referência; o destaque vem da cor do LED.
const BG: Color32 = Color32::from_rgb(0x15, 0x17, 0x1c);
const CARD: Color32 = Color32::from_rgb(0x1f, 0x23, 0x2a);
const CARD_HI: Color32 = Color32::from_rgb(0x29, 0x2e, 0x37);
const TEXT: Color32 = Color32::from_rgb(0xec, 0xee, 0xf2);
const TEXT_2: Color32 = Color32::from_rgb(0xa0, 0xa6, 0xb1);
const TEXT_3: Color32 = Color32::from_rgb(0x6c, 0x72, 0x7d);
const INK: Color32 = Color32::from_rgb(0x12, 0x13, 0x17);
const FALLBACK_ACCENT: Color32 = Color32::from_rgb(0xff, 0x6a, 0x2b);
const OK: Color32 = Color32::from_rgb(0x3d, 0xd6, 0x8c);
const ERR: Color32 = Color32::from_rgb(0xff, 0x5c, 0x5c);

/// Cores rápidas: as cores puras da paleta do app original.
const SWATCHES: [[u8; 3]; 8] = [
    [0xff, 0x00, 0x00],
    [0xff, 0x5a, 0x00],
    [0xff, 0xcd, 0x00],
    [0x00, 0xff, 0x00],
    [0x00, 0x96, 0xff],
    [0x00, 0x00, 0xff],
    [0x69, 0x00, 0xff],
    [0xff, 0xff, 0xff],
];

const SEMIBOLD: &str = "outfit-semibold";
const MEDIUM: &str = "outfit-medium";

fn font(size: f32, family: &str) -> FontId {
    FontId::new(size, FontFamily::Name(family.into()))
}

fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Breathing => "Respiração",
        Mode::Spectrum => "Espectro",
        Mode::Static => "Estático",
        Mode::Wave => "Onda",
        Mode::Marquee => "Letreiro",
        Mode::Heartbeat => "Batimento",
        Mode::Off => "Desligado",
    }
}

fn mode_blurb(m: Mode) -> &'static str {
    match m {
        Mode::Breathing => "A cor acende e apaga devagar, alternando com a cor 2 se ela estiver ligada.",
        Mode::Spectrum => "Todos os LEDs percorrem o arco-íris juntos.",
        Mode::Static => "Uma cor fixa, sem movimento.",
        Mode::Wave => "O arco-íris corre ao longo da fita.",
        Mode::Marquee => "Blocos de cor andam pela fita, como um letreiro.",
        Mode::Heartbeat => "Dois pulsos rápidos e uma pausa.",
        Mode::Off => "Os LEDs desta zona ficam apagados.",
    }
}

/// `None` = todas as zonas (controle global).
type ZoneSel = Option<u8>;

fn zone_title(z: ZoneSel) -> String {
    match z {
        Some(id) => format!("ARGB{id}"),
        None => "Todas".into(),
    }
}

fn zone_subtitle(z: ZoneSel) -> String {
    match z {
        Some(id) => format!("Conector ARGB {id} · 5 V"),
        None => "Controle global: o mesmo efeito em todos os conectores".into(),
    }
}

// ---------- cor ----------

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

fn scale(c: Color32, k: f32) -> Color32 {
    let m = |x: u8| (f32::from(x) * k.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

fn hue(h: f32) -> Color32 {
    egui::ecolor::Hsva::new(h.rem_euclid(1.0), 1.0, 1.0, 1.0).into()
}

fn luminance(c: Color32) -> f32 {
    let lin = |x: u8| {
        let v = f32::from(x) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Clareia a cor até ter contraste de texto (4.5:1) sobre o fundo.
fn readable_on(c: Color32, bg: Color32) -> Color32 {
    let mut out = c;
    let mut t = 0.0;
    while contrast(out, bg) < 4.5 && t < 1.0 {
        t += 0.1;
        out = lerp(c, Color32::WHITE, t);
    }
    out
}

/// Texto escuro ou claro, o que tiver mais contraste sobre `fill`.
fn ink_for(fill: Color32) -> Color32 {
    if contrast(INK, fill) >= contrast(TEXT, fill) { INK } else { TEXT }
}

// ---------- simulação do efeito (só para a prévia) ----------

/// Período em segundos para o nível de velocidade 0..=15.
fn period(speed: u8) -> f32 {
    6.0 - 4.8 * f32::from(speed) / f32::from(MAX_LEVEL)
}

/// Cor de um LED na posição `x` (0..1) no instante `t`.
fn led_color(e: &ZoneEffect, t: f32, x: f32) -> Color32 {
    let c1 = e.color.map(|c| Color32::from_rgb(c.0, c.1, c.2)).unwrap_or(Color32::WHITE);
    let c2 = e.color2.map(|c| Color32::from_rgb(c.0, c.1, c.2)).unwrap_or(c1);
    let p = period(e.speed.unwrap_or(DEFAULT_SPEED));
    let phase = t / p;
    let bright = 0.18 + 0.82 * f32::from(e.brightness.unwrap_or(DEFAULT_BRIGHTNESS)) / f32::from(MAX_LEVEL);
    let base = match e.mode {
        Mode::Off => return Color32::from_rgb(0x26, 0x29, 0x30),
        Mode::Static => c1,
        Mode::Breathing => {
            let cycle = phase.floor() as i64;
            let c = if e.random {
                hue(cycle as f32 * 0.37)
            } else if cycle % 2 == 0 {
                c1
            } else {
                c2
            };
            scale(c, 0.12 + 0.88 * (0.5 - 0.5 * (phase * TAU).cos()))
        }
        Mode::Spectrum => hue(phase),
        Mode::Wave => {
            let dir = if e.direction == Some(Direction::Right) { -1.0 } else { 1.0 };
            hue(x + dir * phase)
        }
        Mode::Marquee => {
            let dir = if e.direction == Some(Direction::Right) { -1.0 } else { 1.0 };
            if (x * 8.0 + dir * phase * 4.0).rem_euclid(2.0) < 1.0 { c1 } else { c2 }
        }
        Mode::Heartbeat => {
            let f = phase.fract();
            let pulse = |c: f32| (-((f - c) / 0.045).powi(2)).exp();
            let c = if e.random { hue(phase.floor() * 0.37) } else { c1 };
            scale(c, 0.08 + 0.92 * (pulse(0.12) + 0.75 * pulse(0.32)).min(1.0))
        }
    };
    scale(base, bright)
}

// ---------- estado ----------

struct Form {
    mode: Mode,
    color: [u8; 3],
    color2: [u8; 3],
    color2_enabled: bool,
    random: bool,
    brightness: u8,
    speed: u8,
    direction: Direction,
}

impl Form {
    fn from_effect(e: &ZoneEffect) -> Form {
        let c = |c: Rgb| [c.0, c.1, c.2];
        let color = e.color.map(c).unwrap_or([0, 0, 0xff]);
        Form {
            mode: e.mode,
            color,
            color2: e.color2.map(c).unwrap_or(color),
            color2_enabled: e.color2.is_some(),
            random: e.random,
            brightness: e.brightness.unwrap_or(DEFAULT_BRIGHTNESS),
            speed: e.speed.unwrap_or(DEFAULT_SPEED),
            direction: e.direction.unwrap_or_default(),
        }
    }

    /// Só os parâmetros que o modo aceita.
    fn effect(&self) -> ZoneEffect {
        let p = self.mode.params();
        let c = |c: [u8; 3]| Rgb(c[0], c[1], c[2]);
        ZoneEffect {
            mode: self.mode,
            color: p.color1.then(|| c(self.color)),
            color2: (p.color2 && self.color2_enabled).then(|| c(self.color2)),
            random: p.random && self.random,
            brightness: p.brightness.then_some(self.brightness),
            speed: p.speed.then_some(self.speed),
            direction: p.direction.then_some(self.direction),
        }
    }

    /// Cor de destaque da interface: a cor 1 do LED, ou laranja nos modos sem cor.
    fn accent(&self) -> Color32 {
        if self.mode.params().color1 { rgb(self.color) } else { FALLBACK_ACCENT }
    }
}

/// O que está na tela e precisa estar na placa.
#[derive(Clone, PartialEq)]
struct Snapshot {
    zone: ZoneSel,
    effect: ZoneEffect,
    leds: [Option<u8>; 3],
}

/// Espera depois da última mudança antes de gravar (arrastar um slider não grava a cada passo).
const SETTLE_SECS: f64 = 0.3;

struct App {
    path: PathBuf,
    cfg: Config,
    zone: ZoneSel,
    form: Form,
    leds: [Option<u8>; 3],
    status: Option<Result<String, String>>,
    connected: bool,
    /// Último estado enviado (ou carregado do arquivo).
    sent: Snapshot,
    /// Quando o estado da tela deixou de bater com `sent`.
    changed_at: Option<f64>,
    /// Gravação em andamento numa thread; a placa leva ~0,1 s por bloco.
    job: Option<mpsc::Receiver<(&'static str, Result<()>)>>,
    /// Fila do que ainda falta gravar: zonas (`None` = global) e conectores com LED count alterado.
    dirty_zones: BTreeSet<ZoneSel>,
    dirty_leds: BTreeSet<u8>,
}

fn device_connected() -> bool {
    HidApi::new().map(|api| device::present(&api)).unwrap_or(false)
}

impl App {
    fn new(path: PathBuf) -> App {
        let (cfg, status) = match config::load(&path) {
            Ok(cfg) => (cfg, None),
            Err(e) => (Config::default(), Some(Err(format!("{e:#}")))),
        };
        let zone = if cfg.global.is_some() { None } else { Some(1) };
        let form = Form::from_effect(&config::default_effect());
        let sent = Snapshot { zone, effect: form.effect(), leds: [None; 3] };
        let mut app = App {
            path,
            form,
            leds: [None; 3],
            cfg,
            zone,
            status,
            connected: device_connected(),
            sent,
            changed_at: None,
            job: None,
            dirty_zones: BTreeSet::new(),
            dirty_leds: BTreeSet::new(),
        };
        app.load_form();
        app
    }

    fn effect_of(&self, z: ZoneSel) -> ZoneEffect {
        let e = match z {
            None => self.cfg.global.as_ref(),
            Some(id) => self.cfg.zone(id),
        };
        e.cloned().unwrap_or_else(config::default_effect)
    }

    /// Efeito que a zona mostra na barra lateral: o que está sendo editado, senão o gravado.
    fn shown_effect(&self, id: u8) -> ZoneEffect {
        if self.zone.is_none() || self.zone == Some(id) {
            return self.form.effect();
        }
        self.cfg.global.clone().unwrap_or_else(|| self.effect_of(Some(id)))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { zone: self.zone, effect: self.form.effect(), leds: self.leds }
    }

    /// Carrega a zona selecionada do arquivo. Trocar de zona só mostra, não grava.
    fn load_form(&mut self) {
        self.form = Form::from_effect(&self.effect_of(self.zone));
        self.leds = [1, 2, 3].map(|h| self.cfg.led_count.get(h));
        self.sent = self.snapshot();
        self.changed_at = None;
    }

    fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Há algo na tela que ainda não chegou à placa.
    fn pending(&self) -> bool {
        self.busy() || !self.dirty_zones.is_empty() || !self.dirty_leds.is_empty() || self.snapshot() != self.sent
    }

    /// Grava na placa em segundo plano.
    fn spawn(&mut self, done: &'static str, work: impl FnOnce() -> Result<()> + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send((done, work()));
        });
        self.job = Some(rx);
    }

    /// Passa o que mudou na tela para a fila de gravação. Com `now`, só depois de a tela parar de mudar.
    fn commit(&mut self, now: Option<f64>) {
        let cur = self.snapshot();
        if cur == self.sent {
            self.changed_at = None;
            return;
        }
        if let Some(now) = now {
            let since = *self.changed_at.get_or_insert(now);
            if now - since < SETTLE_SECS {
                return;
            }
        }
        let prev = std::mem::replace(&mut self.sent, cur.clone());
        self.changed_at = None;
        if let Err(e) = self.save(&cur) {
            self.status = Some(Err(format!("{e:#}")));
            return;
        }
        if prev.effect != cur.effect || prev.zone != cur.zone {
            self.dirty_zones.insert(cur.zone);
        }
        for h in ZONES.filter(|&h| prev.leds[usize::from(h - 1)] != cur.leds[usize::from(h - 1)]) {
            self.dirty_leds.insert(h);
        }
    }

    /// Chamado a cada quadro: recolhe o resultado da gravação anterior e envia o que estiver na fila.
    fn sync(&mut self, now: f64) {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok((done, res)) => {
                    self.job = None;
                    self.status = Some(res.map(|()| done.to_string()).map_err(|e| format!("{e:#}")));
                    self.connected = device_connected();
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.status = Some(Err("a gravação foi interrompida".into()));
                }
            }
        }
        self.commit(Some(now));
        if self.busy() || (self.dirty_zones.is_empty() && self.dirty_leds.is_empty()) {
            return;
        }
        let cfg = self.cfg.clone();
        let leds: Vec<u8> = std::mem::take(&mut self.dirty_leds).into_iter().collect();
        let zones = std::mem::take(&mut self.dirty_zones);
        self.spawn("Aplicado na placa", move || {
            let dev = Device::open()?;
            for h in leds {
                config::apply(&cfg, &dev, Scope::LedCount(h))?;
            }
            // Com o controle global, um único envio cobre todas as zonas.
            let ids: Vec<u8> = zones.into_iter().flatten().collect();
            if cfg.global.is_some() || !ids.is_empty() {
                config::apply(&cfg, &dev, Scope::Zones(&ids))?;
            }
            Ok(())
        });
    }

    /// Atualiza a configuração com o estado da tela e grava o arquivo.
    fn save(&mut self, s: &Snapshot) -> Result<()> {
        s.effect.record()?;
        let mut cfg = self.cfg.clone();
        match s.zone {
            None => cfg.global = Some(s.effect.clone()),
            Some(id) => {
                cfg.global = None;
                cfg.set_zone(id, s.effect.clone());
            }
        }
        for h in ZONES {
            cfg.led_count.set(h, s.leds[usize::from(h - 1)]);
        }
        config::save(&self.path, &cfg)?;
        self.cfg = cfg;
        Ok(())
    }

    /// Reenvia a configuração inteira (útil depois de um erro ou de reconectar).
    fn reapply(&mut self) {
        if self.busy() {
            return;
        }
        let cur = self.snapshot();
        if let Err(e) = self.save(&cur) {
            self.status = Some(Err(format!("{e:#}")));
            return;
        }
        self.sent = cur;
        self.changed_at = None;
        self.dirty_zones.clear();
        self.dirty_leds.clear();
        let cfg = self.cfg.clone();
        self.spawn("Configuração inteira reaplicada", move || config::apply(&cfg, &Device::open()?, Scope::All));
    }

    fn restore(&mut self) {
        if self.busy() {
            return;
        }
        let cfg = Config::default();
        if let Err(e) = config::save(&self.path, &cfg) {
            self.status = Some(Err(format!("{e:#}")));
            return;
        }
        self.cfg = cfg.clone();
        self.zone = Some(1);
        self.load_form();
        self.dirty_zones.clear();
        self.dirty_leds.clear();
        self.spawn("Perfil padrão aplicado", move || config::apply(&cfg, &Device::open()?, Scope::All));
    }
}

// ---------- primitivas de desenho ----------

fn shadow(ui: &Ui, rect: Rect, radius: u8) {
    let s = Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(90) };
    ui.painter().add(s.as_shape(rect, radius));
}

/// Arredonda uma malha retangular: cobre só a área fora do arco em cada canto com a cor do fundo,
/// e passa um traço fino suavizado na borda para não serrilhar.
fn round_corners(ui: &Ui, rect: Rect, radius: f32, bg: Color32) {
    const STEPS: usize = 12;
    let mut mesh = Mesh::default();
    let corners = [(rect.left_top(), 1.0, 1.0), (rect.right_top(), -1.0, 1.0), (rect.left_bottom(), 1.0, -1.0), (rect.right_bottom(), -1.0, -1.0)];
    for (corner, dx, dy) in corners {
        let center = corner + vec2(dx * radius, dy * radius);
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(corner - vec2(dx, dy), bg);
        for i in 0..=STEPS {
            let a = i as f32 / STEPS as f32 * TAU / 4.0;
            mesh.colored_vertex(center + vec2(-dx * radius * a.cos(), -dy * radius * a.sin()), bg);
        }
        for i in 1..=STEPS as u32 {
            mesh.add_triangle(base, base + i, base + i + 1);
        }
    }
    ui.painter().add(Shape::mesh(mesh));
    ui.painter().rect_stroke(rect, CornerRadius::same(radius as u8), Stroke::new(3.0, bg), StrokeKind::Middle);
}

/// Malha com cores por vértice em grade `cols` × 2 (topo, base).
fn gradient(ui: &Ui, rect: Rect, cols: usize, mut color_at: impl FnMut(f32, bool) -> Color32) {
    let mut mesh = Mesh::default();
    for i in 0..=cols {
        let x = i as f32 / cols as f32;
        let px = rect.left() + rect.width() * x;
        mesh.colored_vertex(pos2(px, rect.top()), color_at(x, true));
        mesh.colored_vertex(pos2(px, rect.bottom()), color_at(x, false));
    }
    for i in 0..cols as u32 {
        let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    ui.painter().add(Shape::mesh(mesh));
}

fn pill(ui: &mut Ui, rect: Rect, id: Id, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    ui.interact(rect, id, sense)
}

/// Chip arredondado; selecionado = preenchido.
fn chip(ui: &mut Ui, rect: Rect, id: Id, label: &str, selected: bool, on_color: bool) -> Response {
    let r = pill(ui, rect, id, true);
    let (fill, text) = match (selected, on_color) {
        (true, _) => (Color32::from_white_alpha(235), INK),
        (false, true) => (Color32::from_black_alpha(if r.hovered() { 150 } else { 110 }), TEXT),
        (false, false) => (if r.hovered() { CARD_HI } else { CARD }, TEXT_2),
    };
    ui.painter().rect_filled(rect, CornerRadius::same(255), fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font(14.0, MEDIUM), text);
    r
}

fn chip_width(ui: &Ui, label: &str) -> f32 {
    ui.fonts_mut(|f| f.layout_no_wrap(label.into(), font(14.0, MEDIUM), TEXT).size().x) + 30.0
}

fn switch(ui: &mut Ui, rect: Rect, id: Id, on: &mut bool, accent: Color32, enabled: bool) {
    let r = pill(ui, rect, id, enabled);
    if r.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(id, *on, 0.16);
    let track = lerp(CARD_HI, accent, t);
    let track = if enabled { track } else { track.gamma_multiply(0.4) };
    ui.painter().rect_filled(rect, CornerRadius::same(255), track);
    let rad = rect.height() / 2.0 - 3.0;
    let cx = rect.left() + 3.0 + rad + t * (rect.width() - 6.0 - 2.0 * rad);
    let knob = if enabled { Color32::WHITE } else { TEXT_3 };
    ui.painter().circle_filled(pos2(cx, rect.center().y), rad, knob);
}

fn swatch(ui: &mut Ui, center: Pos2, id: Id, color: Color32, selected: bool, ring: Color32) -> Response {
    let rad = 13.0;
    let r = pill(ui, Rect::from_center_size(center, Vec2::splat(rad * 2.0 + 6.0)), id, true);
    if selected || r.hovered() {
        let w = if selected { 2.5 } else { 1.5 };
        ui.painter().circle_stroke(center, rad + 4.0, Stroke::new(w, ring));
    }
    ui.painter().circle_filled(center, rad, color);
    ui.painter().circle_stroke(center, rad, Stroke::new(1.0, ring.gamma_multiply(0.22)));
    r
}

/// Marca do app: anel com o espectro.
fn logo(ui: &Ui, center: Pos2, t: f32) {
    let n = 48;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * TAU;
        let a1 = (i + 1) as f32 / n as f32 * TAU;
        let p = |a: f32, r: f32| center + vec2(a.cos(), a.sin()) * r;
        let c = hue(i as f32 / n as f32 + t * 0.05);
        ui.painter().add(Shape::convex_polygon(vec![p(a0, 15.0), p(a1, 15.0), p(a1, 9.0), p(a0, 9.0)], c, Stroke::NONE));
    }
}

// ---------- seções ----------

impl App {
    fn sidebar(&mut self, ui: &mut Ui, rect: Rect, t: f32, compact: bool) {
        let p = ui.painter().clone();
        logo(ui, pos2(rect.left() + 18.0, rect.top() + 22.0), t);
        let brand = if compact { "SOYO" } else { "SOYO ARGB" };
        p.text(pos2(rect.left() + 44.0, rect.top() + 22.0), Align2::LEFT_CENTER, brand, font(19.0, SEMIBOLD), TEXT);

        let accent = readable_on(self.form.accent(), BG);
        let mut y = rect.top() + 104.0;
        for z in ZONES.map(Some).chain([None]) {
            let item = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 44.0));
            let r = pill(ui, item, Id::unique(("nav", z)), true);
            let selected = self.zone == z;
            if r.clicked() && !selected {
                self.commit(None);
                self.zone = z;
                self.load_form();
            }
            if r.hovered() && !selected {
                p.rect_filled(item, CornerRadius::same(12), Color32::from_white_alpha(6));
            }
            if selected {
                let bar = Rect::from_min_size(pos2(rect.left() - 18.0, y + 10.0), vec2(4.0, 24.0));
                p.rect_filled(bar, CornerRadius::same(2), accent);
            }
            // Bolinha com a cor (ou prévia) atual da zona.
            let dot = pos2(item.left() + 22.0, item.center().y);
            match z {
                Some(id) => {
                    let e = self.shown_effect(id);
                    if e.mode == Mode::Off {
                        p.circle_stroke(dot, 7.0, Stroke::new(1.5, TEXT_3));
                    } else {
                        let c = led_color(&e, t, 0.3);
                        p.circle_filled(dot, 10.0, c.gamma_multiply(0.25));
                        p.circle_filled(dot, 6.5, c);
                    }
                }
                None => {
                    for (k, off) in [-5.0f32, 0.0, 5.0].iter().enumerate() {
                        p.circle_filled(dot + vec2(*off, 0.0), 4.0, hue(k as f32 / 3.0 + t * 0.1));
                    }
                }
            }
            let color = if selected { accent } else { TEXT_2 };
            let f = if selected { font(16.0, SEMIBOLD) } else { font(16.0, MEDIUM) };
            p.text(pos2(item.left() + 44.0, item.center().y), Align2::LEFT_CENTER, zone_title(z), f, color);
            y += 50.0;
        }

        // Card do dispositivo, embaixo.
        let card = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 92.0), rect.max);
        p.rect_filled(card, CornerRadius::same(18), CARD);
        let (dot, label) = if self.connected { (OK, "Conectado") } else { (ERR, "Não encontrado") };
        p.text(card.left_top() + vec2(18.0, 22.0), Align2::LEFT_CENTER, "Controlador", font(15.0, MEDIUM), TEXT);
        p.circle_filled(card.left_top() + vec2(22.0, 50.0), 4.0, dot);
        p.text(card.left_top() + vec2(32.0, 50.0), Align2::LEFT_CENTER, label, regular(14.0), TEXT_2);
        if !compact {
            p.text(card.left_top() + vec2(18.0, 72.0), Align2::LEFT_CENTER, "SONiX 0c45:7125", regular(13.0), TEXT_3);
        }
        let refresh_at = if compact { card.right_bottom() + vec2(-26.0, -24.0) } else { card.right_top() + vec2(-28.0, 28.0) };
        let refresh = Rect::from_center_size(refresh_at, Vec2::splat(28.0));
        let r = pill(ui, refresh, Id::unique("refresh"), true).on_hover_text("Procurar o controlador de novo");
        p.circle_stroke(refresh.center(), 13.0, Stroke::new(1.0, if r.hovered() { TEXT_2 } else { CARD_HI }));
        // Seta circular desenhada (atualizar).
        let c = refresh.center();
        let pts: Vec<Pos2> = (0..=20).map(|i| {
            let a = 0.3 + i as f32 / 20.0 * 4.9;
            c + vec2(a.cos(), a.sin()) * 6.0
        }).collect();
        let tip = *pts.last().unwrap();
        p.add(Shape::line(pts, Stroke::new(1.6, TEXT_2)));
        p.add(Shape::convex_polygon(vec![tip + vec2(-3.5, -1.5), tip + vec2(2.5, -3.5), tip + vec2(1.5, 2.5)], TEXT_2, Stroke::NONE));
        if r.clicked() {
            self.connected = device_connected();
        }
    }

    fn top_bar(&self, ui: &Ui, rect: Rect) {
        let p = ui.painter();
        p.text(rect.left_top() + vec2(0.0, 4.0), Align2::LEFT_TOP, zone_title(self.zone), font(28.0, SEMIBOLD), TEXT);
        p.text(rect.left_top() + vec2(0.0, 40.0), Align2::LEFT_TOP, zone_subtitle(self.zone), regular(14.0), TEXT_2);
        let pending = self.pending().then(|| (TEXT_2, "Aplicando…".to_string()));
        let shown = pending.or_else(|| match &self.status {
            Some(Ok(m)) => Some((OK, m.clone())),
            Some(Err(m)) => Some((ERR, m.clone())),
            None => None,
        });
        if let Some((dot, msg)) = shown {
            let max_w = rect.width() * 0.6;
            let galley = ui.fonts_mut(|f| f.layout(msg.to_string(), regular(14.0), TEXT, max_w));
            let size = galley.size() + vec2(44.0, 18.0);
            let r = Rect::from_min_size(pos2(rect.right() - size.x, rect.top() + 4.0), size);
            p.rect_filled(r, CornerRadius::same(14), CARD);
            p.circle_filled(r.left_center() + vec2(18.0, 0.0), 4.0, dot);
            p.galley(r.left_top() + vec2(30.0, 9.0), galley, TEXT);
        }
    }

    fn hero(&mut self, ui: &mut Ui, rect: Rect, t: f32) {
        let e = self.form.effect();
        // Fundo: a própria fita, esticada, escurecendo para baixo e para a esquerda (leitura do texto).
        gradient(ui, rect, 48, |x, top| {
            let c = led_color(&e, t, x);
            let glow = if top { 0.85 } else { 0.38 };
            lerp(Color32::from_rgb(0x18, 0x1a, 0x20), c, glow * (0.35 + 0.65 * x))
        });
        gradient(ui, rect, 12, |x, _| Color32::from_black_alpha((210.0 * (1.0 - x / 0.62).clamp(0.0, 1.0)) as u8));
        round_corners(ui, rect, 22.0, BG);

        // Chips de modo.
        let mut x = rect.left() + 22.0;
        let mut y = rect.top() + 22.0;
        for m in Mode::ALL {
            let w = chip_width(ui, mode_label(m));
            if x + w > rect.right() - 22.0 {
                x = rect.left() + 22.0;
                y += 40.0;
            }
            let r = Rect::from_min_size(pos2(x, y), vec2(w, 32.0));
            if chip(ui, r, Id::unique(("mode", m.name())), mode_label(m), self.form.mode == m, true).clicked() {
                self.form.mode = m;
            }
            x += w + 8.0;
        }

        let p = ui.painter();
        let left = rect.left() + 28.0;
        let title_y = (rect.center().y + 6.0).max(y + 32.0 + 64.0);
        p.text(pos2(left, title_y), Align2::LEFT_BOTTOM, mode_label(self.form.mode), font(48.0, SEMIBOLD), Color32::WHITE);
        let blurb = ui.fonts_mut(|f| {
            f.layout(mode_blurb(self.form.mode).into(), regular(15.0), Color32::from_white_alpha(215), (rect.width() * 0.55).max(220.0))
        });
        p.galley(pos2(left, title_y + 10.0), blurb, Color32::WHITE);

        // Fita de LEDs no rodapé, como a fileira de avatares da referência.
        let count = match self.zone {
            Some(id) => self.leds[usize::from(id - 1)].unwrap_or(30),
            None => 30,
        };
        let strip = Rect::from_min_size(pos2(left - 6.0, rect.bottom() - 58.0), vec2((rect.width() * 0.62).min(520.0), 36.0));
        p.rect_filled(strip, CornerRadius::same(255), Color32::from_black_alpha(120));
        let label = format!("{count} LEDs");
        let lw = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font(13.0, MEDIUM), TEXT).size().x);
        p.text(pos2(strip.right() - 16.0, strip.center().y), Align2::RIGHT_CENTER, &label, font(13.0, MEDIUM), TEXT);
        let span = strip.width() - lw - 48.0;
        let n = usize::from(count).min((span / 6.0) as usize).max(2);
        for i in 0..n {
            let fx = i as f32 / (n - 1) as f32;
            let c = led_color(&e, t, fx);
            let pos = pos2(strip.left() + 18.0 + fx * span, strip.center().y);
            p.circle_filled(pos, 2.6, c);
            if e.mode != Mode::Off {
                p.circle_filled(pos, 5.0, c.gamma_multiply(0.18));
            }
        }
    }

    fn colors_card(&mut self, ui: &mut Ui, rect: Rect) {
        let params = self.form.mode.params();
        let c1 = self.form.accent();
        let c2 = if params.color2 && self.form.color2_enabled { rgb(self.form.color2) } else { scale(c1, 0.55) };
        gradient(ui, rect, 16, |x, top| lerp(c1, c2, x * 0.8 + if top { 0.0 } else { 0.2 }));
        round_corners(ui, rect, 22.0, BG);

        let avg = lerp(c1, c2, 0.5);
        let ink = ink_for(avg);
        let ink2 = ink.gamma_multiply(0.78);
        let p = ui.painter().clone();
        let mut y = rect.top() + 26.0;
        p.text(pos2(rect.left() + 24.0, y), Align2::LEFT_TOP, "Cores", font(26.0, SEMIBOLD), ink);
        y += 44.0;

        if !params.color1 {
            let g = ui.fonts_mut(|f| {
                f.layout(
                    "Este modo usa o arco-íris do próprio controlador; não há cor para escolher.".into(),
                    regular(15.0),
                    ink2,
                    rect.width() - 48.0,
                )
            });
            p.galley(pos2(rect.left() + 24.0, y), g, ink2);
        } else {
            let ring = ink;
            let gap = (rect.width() - 48.0) / 4.0;
            for (i, s) in SWATCHES.iter().enumerate() {
                let c = pos2(rect.left() + 24.0 + gap * ((i % 4) as f32 + 0.5), y + 16.0 + 40.0 * (i / 4) as f32);
                if swatch(ui, c, Id::unique(("sw", i)), rgb(*s), self.form.color == *s, ring).clicked() {
                    self.form.color = *s;
                }
            }
            y += 92.0;
            let row = |y: f32| Rect::from_min_size(pos2(rect.left() + 24.0, y), vec2(rect.width() - 48.0, 30.0));
            p.text(row(y).left_center(), Align2::LEFT_CENTER, "Outra cor", font(15.0, MEDIUM), ink);
            ui.scope_builder(UiBuilder::new().max_rect(Rect::from_min_size(row(y).right_top() - vec2(44.0, -2.0), vec2(44.0, 26.0))), |ui| {
                ui.spacing_mut().interact_size = vec2(44.0, 26.0);
                egui::color_picker::color_edit_button_srgb(ui, &mut self.form.color);
            });
            y += 40.0;
            let accent = rgb(self.form.color);
            p.text(row(y).left_center(), Align2::LEFT_CENTER, "Cor 2", font(15.0, MEDIUM), if params.color2 { ink } else { ink.gamma_multiply(0.4) });
            let sw = Rect::from_min_size(row(y).right_top() + vec2(-46.0, 3.0), vec2(46.0, 24.0));
            switch(ui, sw, Id::unique("color2"), &mut self.form.color2_enabled, ink_for(accent), params.color2);
            if params.color2 && self.form.color2_enabled {
                ui.scope_builder(UiBuilder::new().max_rect(Rect::from_min_size(sw.left_top() - vec2(56.0, 1.0), vec2(44.0, 26.0))), |ui| {
                    ui.spacing_mut().interact_size = vec2(44.0, 26.0);
                    egui::color_picker::color_edit_button_srgb(ui, &mut self.form.color2);
                });
            }
            y += 40.0;
            p.text(row(y).left_center(), Align2::LEFT_CENTER, "Aleatória", font(15.0, MEDIUM), if params.random { ink } else { ink.gamma_multiply(0.4) });
            let sw = Rect::from_min_size(row(y).right_top() + vec2(-46.0, 3.0), vec2(46.0, 24.0));
            switch(ui, sw, Id::unique("random"), &mut self.form.random, ink_for(accent), params.random);
        }

        // Botão de estado: grava sozinho; clicar reenvia tudo.
        let btn = Rect::from_min_max(pos2(rect.left() + 20.0, rect.bottom() - 74.0), pos2(rect.right() - 20.0, rect.bottom() - 20.0));
        let pending = self.pending();
        let failed = matches!(self.status, Some(Err(_)));
        let r = pill(ui, btn, Id::unique("apply"), !pending)
            .on_hover_text("As mudanças vão para a placa sozinhas. Clique para reenviar a configuração inteira.");
        let fill = Color32::from_black_alpha(if r.is_pointer_button_down_on() { 150 } else if r.hovered() { 120 } else { 90 });
        p.rect_filled(btn, CornerRadius::same(16), fill);
        let (label, dot) = match (pending, failed) {
            (true, _) => ("Aplicando…", None),
            (false, true) => ("Tentar de novo", Some(ERR)),
            (false, false) => ("Aplicado na placa", Some(OK)),
        };
        let text = p.text(btn.center() + vec2(8.0, 0.0), Align2::CENTER_CENTER, label, font(18.0, MEDIUM), Color32::WHITE);
        if let Some(dot) = dot {
            p.circle_filled(pos2(text.left() - 14.0, btn.center().y), 4.5, dot);
        }
        if r.clicked() {
            self.reapply();
        }
    }

    fn adjust_cards(&mut self, ui: &mut Ui, rect: Rect) {
        let params = self.form.mode.params();
        let accent = readable_on(self.form.accent(), CARD);
        let weights = [1.0f32, 1.3, 1.1];
        let gap = 16.0;
        let unit = (rect.width() - gap * 2.0) / weights.iter().sum::<f32>();
        let mut x = rect.left();
        let mut cards = [Rect::NOTHING; 3];
        for (i, w) in weights.iter().enumerate() {
            cards[i] = Rect::from_min_size(pos2(x, rect.top()), vec2(unit * w, rect.height()));
            x += unit * w + gap;
        }
        for c in cards {
            shadow(ui, c, 20);
            ui.painter().rect_filled(c, CornerRadius::same(20), CARD);
        }

        ui.visuals_mut().selection.bg_fill = accent;
        self.level_card(ui, cards[0], "Brilho", ("Apagado", "Máximo"), params.brightness, true);
        self.level_card(ui, cards[1], "Velocidade", ("Lenta", "Rápida"), params.speed, false);

        // Direção, no mesmo card da velocidade (os dois descrevem o movimento).
        let c = cards[1];
        let p = ui.painter().clone();
        let on = params.direction;
        let seg = Rect::from_min_size(c.left_top() + vec2(18.0, 152.0), vec2(c.width() - 36.0, 42.0));
        p.rect_filled(seg, CornerRadius::same(14), BG);
        let half = seg.width() / 2.0;
        for (i, (d, label)) in [(Direction::Left, "Para a esquerda"), (Direction::Right, "Para a direita")].into_iter().enumerate() {
            let r = Rect::from_min_size(seg.left_top() + vec2(4.0 + half * i as f32, 4.0), vec2(half - 8.0, seg.height() - 8.0));
            let resp = pill(ui, r, Id::unique(("dir", i)), on);
            if resp.clicked() {
                self.form.direction = d;
            }
            let sel = self.form.direction == d;
            if sel && on {
                p.rect_filled(r, CornerRadius::same(11), accent);
            }
            let color = match (on, sel) {
                (false, _) => TEXT_3,
                (true, true) => ink_for(accent),
                (true, false) if resp.hovered() => TEXT,
                _ => TEXT_2,
            };
            p.text(r.center(), Align2::CENTER_CENTER, label, font(14.0, MEDIUM), color);
        }

        // LEDs por conector.
        let c = cards[2];
        let p = ui.painter().clone();
        p.text(c.left_top() + vec2(22.0, 24.0), Align2::LEFT_TOP, "LEDs por conector", font(17.0, SEMIBOLD), TEXT);
        let mut y = c.top() + 66.0;
        for h in ZONES {
            let row = Rect::from_min_size(pos2(c.left() + 22.0, y), vec2(c.width() - 44.0, 30.0));
            let slot = &mut self.leds[usize::from(h - 1)];
            let mut set = slot.is_some();
            let sw = Rect::from_min_size(row.left_top() + vec2(0.0, 4.0), vec2(40.0, 22.0));
            switch(ui, sw, Id::unique(("ledsw", h)), &mut set, accent, true);
            p.text(pos2(sw.right() + 12.0, row.center().y), Align2::LEFT_CENTER, format!("ARGB{h}"), font(15.0, MEDIUM), if set { TEXT } else { TEXT_2 });
            match (set, slot.as_mut()) {
                (true, Some(n)) => {
                    let field = Rect::from_min_size(pos2(row.right() - 64.0, row.top()), vec2(64.0, 30.0));
                    ui.put(field, egui::DragValue::new(n).range(LED_COUNT_MIN..=LED_COUNT_MAX).speed(0.2));
                }
                (true, None) => *slot = Some(30),
                (false, _) => {
                    *slot = None;
                    p.text(pos2(row.right(), row.center().y), Align2::RIGHT_CENTER, "não enviar", regular(13.0), TEXT_3);
                }
            }
            y += 40.0;
        }
    }

    fn level_card(&mut self, ui: &mut Ui, c: Rect, title: &str, ends: (&str, &str), on: bool, brightness: bool) {
        let p = ui.painter().clone();
        let value = if brightness { &mut self.form.brightness } else { &mut self.form.speed };
        p.text(c.left_top() + vec2(22.0, 24.0), Align2::LEFT_TOP, title, font(17.0, SEMIBOLD), if on { TEXT } else { TEXT_3 });
        let shown = if on { value.to_string() } else { "—".into() };
        p.text(c.right_top() + vec2(-22.0, 14.0), Align2::RIGHT_TOP, shown, font(38.0, SEMIBOLD), if on { TEXT } else { TEXT_3 });
        let slider = Rect::from_min_size(c.left_top() + vec2(20.0, 82.0), vec2(c.width() - 40.0, 28.0));
        ui.scope_builder(UiBuilder::new().max_rect(slider), |ui| {
            ui.spacing_mut().slider_width = slider.width();
            ui.spacing_mut().slider_rail_height = 6.0;
            ui.add_enabled(
                on,
                egui::Slider::new(value, 0..=MAX_LEVEL)
                    .show_value(false)
                    .trailing_fill(true)
                    .handle_shape(egui::style::HandleShape::Circle),
            );
        });
        let ends_y = slider.bottom() + 12.0;
        if on {
            p.text(pos2(slider.left(), ends_y), Align2::LEFT_TOP, ends.0, regular(13.0), TEXT_3);
            p.text(pos2(slider.right(), ends_y), Align2::RIGHT_TOP, ends.1, regular(13.0), TEXT_3);
        } else {
            p.text(pos2(slider.left(), ends_y), Align2::LEFT_TOP, "Não se aplica a este modo", regular(13.0), TEXT_3);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let t = ui.input(|i| i.time) as f32;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| {
            let full = ui.max_rect();
            let m = 28.0;
            let compact = full.width() < 1100.0;
            let side_w = if compact { 132.0 } else { 196.0 };
            let sidebar = Rect::from_min_max(full.min + vec2(m + 18.0, m), pos2(full.left() + m + side_w, full.bottom() - m));
            let content = Rect::from_min_max(pos2(sidebar.right() + 32.0, full.top() + m), full.max - vec2(m, m));

            self.sidebar(ui, sidebar, t, compact);

            let top = Rect::from_min_size(content.min, vec2(content.width(), 64.0));
            self.top_bar(ui, top);

            const CARDS_H: f32 = 212.0;
            let hero_h = (content.height() - 84.0 - 60.0 - CARDS_H).clamp(300.0, 560.0);
            let right_w = (content.width() * 0.3).clamp(290.0, 360.0);
            let row_top = top.bottom() + 20.0;
            let hero = Rect::from_min_size(pos2(content.left(), row_top), vec2(content.width() - right_w - 18.0, hero_h));
            let colors = Rect::from_min_size(pos2(hero.right() + 18.0, row_top), vec2(right_w, hero_h));
            self.hero(ui, hero, t);
            self.colors_card(ui, colors);

            // Cabeçalho dos ajustes, com o restaurar à direita (como o "Learn more").
            let head_y = hero.bottom() + 34.0;
            let p = ui.painter().clone();
            p.text(pos2(content.left(), head_y), Align2::LEFT_CENTER, "Ajustes", font(19.0, SEMIBOLD), TEXT);
            let restore = Rect::from_min_size(pos2(content.right() - 132.0, head_y - 14.0), vec2(132.0, 28.0));
            let busy = self.busy();
            let r = pill(ui, restore, Id::unique("restore"), !busy).on_hover_text("Volta ao perfil padrão do app original e aplica");
            p.text(restore.right_center(), Align2::RIGHT_CENTER, "Restaurar padrão", font(15.0, MEDIUM), if r.hovered() { TEXT } else { TEXT_2 });
            if r.clicked() {
                self.restore();
            }

            let cards = Rect::from_min_size(pos2(content.left(), head_y + 26.0), vec2(content.width(), CARDS_H));
            self.adjust_cards(ui, cards);
        });
        self.sync(ui.input(|i| i.time));
        // A prévia anima em ~30 quadros por segundo; isso também mantém a fila de gravação andando.
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 3] = [
        ("outfit-regular", include_bytes!("../../assets/fonts/Outfit-Regular.ttf")),
        (MEDIUM, include_bytes!("../../assets/fonts/Outfit-Medium.ttf")),
        (SEMIBOLD, include_bytes!("../../assets/fonts/Outfit-SemiBold.ttf")),
    ];
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    for (name, data) in faces {
        fonts.font_data.insert(name.into(), FontData::from_static(data).into());
        let mut chain = vec![name.to_string()];
        chain.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), chain);
    }
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "outfit-regular".into());
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = BG;
        v.window_corner_radius = CornerRadius::same(16);
        v.window_stroke = Stroke::new(1.0, CARD_HI);
        v.override_text_color = Some(TEXT);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(10);
            w.bg_stroke = Stroke::NONE;
        }
        v.widgets.inactive.bg_fill = CARD_HI;
        v.widgets.inactive.weak_bg_fill = BG;
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x35, 0x3b, 0x46);
        v.widgets.hovered.weak_bg_fill = CARD_HI;
        v.widgets.active.bg_fill = Color32::WHITE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
        v.selection.stroke = Stroke::new(1.0, TEXT);
        s.spacing.interact_size.y = 28.0;
    });
}

fn main() -> eframe::Result {
    let path = match std::env::args_os().nth(1) {
        Some(p) => PathBuf::from(p),
        None => config::default_path().unwrap_or_else(|e| {
            eprintln!("erro: {e:#}");
            std::process::exit(1);
        }),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SOYO ARGB")
            .with_app_id("soyo-argb")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([1040.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "soyo-argb-gui",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(App::new(path)))
        }),
    )
}
