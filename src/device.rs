//! Acesso ao controlador pelo hidraw. Só sequências de `docs/protocol.md` §2.

use std::fs::File;
use std::io::Read;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use hidapi::{HidApi, HidDevice};

use crate::protocol::{
    self, BlockRead, BlockWrite, COMMAND_DELAY_MS, COMMAND_RETRIES, PID, REPORT_LEN, Report, VID,
};

pub struct Device {
    hid: HidDevice,
    pub path: String,
}

fn delay() {
    sleep(Duration::from_millis(COMMAND_DELAY_MS));
}

fn hex(r: &[u8]) -> String {
    r.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// `true` se o controlador está conectado (usa uma enumeração já feita).
pub fn present(api: &HidApi) -> bool {
    api.device_list().any(|d| d.vendor_id() == VID && d.product_id() == PID)
}

impl Device {
    pub fn open() -> Result<Device> {
        let api = HidApi::new().context("falha ao iniciar o hidapi")?;
        let info = api
            .device_list()
            .find(|d| d.vendor_id() == VID && d.product_id() == PID)
            .ok_or_else(|| anyhow!("dispositivo SONiX {VID:04x}:{PID:04x} não encontrado"))?;
        let path = info.path().to_string_lossy().into_owned();
        let hid = info.open_device(&api).map_err(|e| {
            anyhow!(
                "sem acesso a {path} ({e}). Instale a regra udev: \
                 sudo install -m644 packaging/60-soyo-argb.rules /etc/udev/rules.d/ && \
                 sudo udevadm control --reload && sudo udevadm trigger --action=change --subsystem-match=hidraw"
            )
        })?;
        Ok(Device { hid, path })
    }

    fn set(&self, r: &Report) -> Result<()> {
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[1..].copy_from_slice(r);
        self.hid
            .send_feature_report(&buf)
            .with_context(|| format!("falha no SET_REPORT {}", hex(&r[..9])))
    }

    fn get(&self) -> Result<Report> {
        let mut buf = [0u8; REPORT_LEN + 1];
        let n = self.hid.get_feature_report(&mut buf).context("falha no GET_REPORT")?;
        if n < REPORT_LEN {
            bail!("GET_REPORT curto: {n} bytes");
        }
        let mut r = [0u8; REPORT_LEN];
        r.copy_from_slice(&buf[1..]);
        Ok(r)
    }

    /// SET + espera + GET, até o ack (`resp[3] == 1`).
    fn command(&self, r: &Report) -> Result<Report> {
        let mut last = [0u8; REPORT_LEN];
        for _ in 0..COMMAND_RETRIES {
            self.set(r)?;
            delay();
            last = self.get()?;
            if protocol::is_ack(&last) {
                return Ok(last);
            }
        }
        bail!("sem ack para {}: resposta {}", hex(&r[..9]), hex(&last[..9]))
    }

    fn sum(&self) -> Result<u32> {
        delay();
        let resp = self.command(&protocol::sum_command())?;
        Ok(protocol::sum_value(&resp))
    }

    /// Desafio `0xab` que o app original faz antes de gravar um registro.
    fn verify(&self) -> Result<()> {
        let mut r = [0u8; REPORT_LEN];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut r))
            .context("falha ao ler /dev/urandom")?;
        self.command(&protocol::verify_command())?;
        delay();
        self.set(&r)?;
        let got = self.sum()?;
        let want = protocol::verify_response(&r);
        if got != want {
            bail!("o dispositivo não passou na verificação 0xab: esperado {want:08x}, recebido {got:08x}");
        }
        Ok(())
    }

    fn write(&self, w: &BlockWrite) -> Result<()> {
        self.command(&w.header)?;
        delay();
        self.set(&w.block)?;
        let got = self.sum()?;
        if got != w.expected_sum() {
            bail!(
                "soma divergente após gravar {}: esperado {:08x}, recebido {got:08x}",
                hex(&w.header[..9]),
                w.expected_sum()
            );
        }
        delay();
        Ok(())
    }

    fn read(&self, rd: &BlockRead) -> Result<Vec<Report>> {
        self.command(&rd.header)?;
        let mut blocks = Vec::with_capacity(rd.blocks);
        for _ in 0..rd.blocks {
            delay();
            blocks.push(self.get()?);
        }
        let got = self.sum()?;
        let want = blocks.iter().map(|b| protocol::checksum(b)).sum::<u32>();
        if got != want {
            bail!("soma divergente na leitura {}: esperado {want:08x}, recebido {got:08x}", hex(&rd.header[..9]));
        }
        delay();
        Ok(blocks)
    }

    /// Grava um registro de iluminação (zona ou global), precedido do desafio `0xab`.
    pub fn write_effect(&self, w: &BlockWrite) -> Result<()> {
        self.verify()?;
        self.write(w)
    }

    /// Grava um bloco que o app envia sem desafio (controle global, quantidade de LEDs).
    pub fn write_plain(&self, w: &BlockWrite) -> Result<()> {
        self.write(w)
    }

    pub fn read_one(&self, rd: &BlockRead) -> Result<Report> {
        Ok(self.read(rd)?[0])
    }

    pub fn read_all(&self, rd: &BlockRead) -> Result<Vec<Report>> {
        self.read(rd)
    }

    /// Envia um Feature report arbitrário; com `read`, devolve o GET seguinte.
    pub fn raw(&self, r: &Report, read: bool) -> Result<Option<Report>> {
        self.set(r)?;
        if !read {
            return Ok(None);
        }
        delay();
        self.get().map(Some)
    }
}

pub fn hex_report(r: &[u8]) -> String {
    hex(r)
}
