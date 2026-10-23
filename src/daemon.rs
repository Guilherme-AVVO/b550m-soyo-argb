//! Reaplica a configuração na partida, quando o dispositivo reaparece e ao voltar da suspensão.
//! Não observa o arquivo de configuração: a CLI e a GUI aplicam diretamente.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use hidapi::HidApi;

use crate::config::{self, Scope};
use crate::device::{self, Device};
use crate::protocol::ARRIVAL_DELAY_MS;

const POLL: Duration = Duration::from_secs(2);
/// Quanto o relógio BOOTTIME pode avançar além do MONOTONIC antes de contarmos como suspensão.
const SUSPEND_THRESHOLD: Duration = Duration::from_secs(5);

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn clock(id: libc::clockid_t) -> Duration {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` é um timespec válido e o relógio existe no Linux.
    unsafe { libc::clock_gettime(id, &mut ts) };
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

fn apply(path: &Path, why: &str) {
    let result = config::load(path).and_then(|cfg| config::apply(&cfg, &Device::open()?, Scope::All));
    match result {
        Ok(()) => eprintln!("configuração aplicada ({why})"),
        Err(e) => eprintln!("falha ao aplicar ({why}): {e:#}"),
    }
}

pub fn run(path: &Path) -> Result<()> {
    // SAFETY: o handler só grava num AtomicBool.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    let mut api = HidApi::new().context("falha ao iniciar o hidapi")?;
    let mut was_present = device::present(&api);
    if was_present {
        apply(path, "partida");
    } else {
        eprintln!("dispositivo ausente; aguardando");
    }
    let mut boot = clock(libc::CLOCK_BOOTTIME);
    let mut mono = clock(libc::CLOCK_MONOTONIC);
    while !STOP.load(Ordering::Relaxed) {
        sleep(POLL);
        if let Err(e) = api.refresh_devices() {
            eprintln!("falha ao enumerar dispositivos: {e}");
            continue;
        }
        let present = device::present(&api);
        let (b, m) = (clock(libc::CLOCK_BOOTTIME), clock(libc::CLOCK_MONOTONIC));
        let suspended = (b - boot).saturating_sub(m - mono) > SUSPEND_THRESHOLD;
        (boot, mono) = (b, m);
        if present && !was_present {
            sleep(Duration::from_millis(ARRIVAL_DELAY_MS));
            apply(path, "dispositivo conectado");
        } else if present && suspended {
            sleep(Duration::from_millis(ARRIVAL_DELAY_MS));
            apply(path, "retorno da suspensão");
        } else if !present && was_present {
            eprintln!("dispositivo removido");
        }
        was_present = present;
    }
    eprintln!("encerrando");
    Ok(())
}
