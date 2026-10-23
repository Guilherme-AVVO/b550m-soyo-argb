# Engenharia reversa — artefatos

Todas as Evidences do REA em `re/evidence/` e as conclusões em `re/findings.md` se referem
exatamente a estes binários. O instalador `ARGB.zip` e `re/evidence/` não vão para o repositório público
(são da SOYO ou contêm código decompilado dela). Com o `ARGB.zip` na raiz, recrie `re/work/` com:

```sh
7z x ARGB.zip -ore/work/zip
7z x "re/work/zip/ARGB/梅捷B550ARGB灯控软件.exe" -ore/work/app
```

O instalador é NSIS 3.10 Unicode (LZMA); o `7z` extrai sem executar nada.

| Arquivo | sha256 |
|---|---|
| `ARGB.zip` | `3cd37943fa8873c95a3cf855a1d1356382634d045adcbd861901b5e210a3c6c0` |
| `re/work/zip/ARGB/梅捷B550ARGB灯控软件.exe` | `e0035c8fe3bd17f453eda3b6430f433f90cc29e711c3233487a27c01ac4f889b` |
| `re/work/app/GamingLightControl.exe` | `ff5c8ea4f64ecc16327fa332ea4ec0f918293083a699cd1c6c023607781750c2` |
| `re/work/app/SafeClose.exe` | `90fc6e8aefc4f0e704ce8f7bead3a1899b74a26b72177fae800c4188087f77e8` |
| `re/work/app/Bin/GL/GamingLightControlWriter.dll` | `f7246757714367a37770d414660553e26bffc4fb13fcba63e95c08e0fd87a81c` |
| `re/work/app/Bin/GL/GamingLightControlMain.dll` | `f5851c2bfbf4d61e769af84d4654cad16f16c30773c24aa7c53568e531d137b2` |
| `re/work/app/Bin/GL/SonixHidDll.dll` | `3f627c5b03c8ab314ef26aa33e8e46294c5b49f30a89c29c09bd4743f213065f` |

## Captura USB

As capturas em `captures/` foram feitas com `dumpcap -i usbmon1` enquanto o app rodava no Wine 11.20
(`WINEPREFIX=re/work/wineprefix`, `Language=en-US` em `re/work/app/GamingLightControl.ini`).
O `dumpcap` descarta `CAP_DAC_OVERRIDE` antes de abrir o usbmon, então o nó precisa ser legível pelo grupo
`wireshark`: `sudo install -m644 re/61-usbmon-wireshark.rules /etc/udev/rules.d/` (e `sudo modprobe usbmon`).
Cenários e horários: `captures/SCENARIOS.md`.
