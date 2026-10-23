# Achados — engenharia reversa do controle ARGB SOYO

Binários: ver hashes em `re/README.md`. Ferramenta: REA 6.7.0 + Ghidra 12.1.2.
O REA descarta as Evidences ao fechar um alvo, então há um bundle por binário em
`re/evidence/static-*.json` (Evidence IDs abaixo estão nesses arquivos). Os bundles têm código decompilado
da SOYO e não são publicados no repositório.

Endereços são do Ghidra (imagem base padrão do PE). "Writer" = `Bin/GL/GamingLightControlWriter.dll`.

## Transporte (`SonixHidDll.dll`, `static-sonixhiddll.json`, Evidence `ev_6a919f0e…`)

| Pergunta | Resposta | Tipo | Lacunas |
|---|---|---|---|
| `HIDReportOut(idx, buf, len)` @0x100019d0 | Abre o caminho HID com `CreateFileA`, monta `[0x00] + buf` (Report ID 0) com tamanho `FeatureReportByteLength` (65) e chama `HidD_SetFeature`. **SET_REPORT Feature no ep0.** Retorna 1 em sucesso. `__stdcall`, sem timeout. | observado | — |
| `HIDReportIn(idx, buf, len)` @0x10001b00 | `HidD_GetFeature` com Report ID 0, copia até `len` bytes **depois** do byte de Report ID. **GET_REPORT Feature.** | observado | — |
| `HIDReportOut2(idx, buf, len, timeout)` @0x10001f00 | `WriteFile` overlapped com `[0x00] + buf`, tamanho `OutputReportByteLength`. **Interrupt OUT (ep 0x02).** | observado | — |
| `HIDReportIn2` @0x10001c00 | `ReadFile` com Report ID 5 numa coleção diferente. Não se aplica ao 0c45:7125 (descriptor sem Report ID). | observado | — |
| `HIDReportIn3(idx, buf, len, timeout)` @0x10001d80 | `ReadFile` overlapped, Report ID 0. **Interrupt IN (ep 0x81).** | observado | — |
| `FindDeviceWithID(vid, pid, kind)` @0x10001030 | `SetupDi*` + `HidD_GetAttributes`; guarda os caminhos das coleções Feature/Input/Output. | observado | — |

## Writer — ligação com a DLL de transporte (`static-writer.json`)

- Loader `FUN_10003c40` (Evidence `ev_905fc7ab…`): `LoadLibraryW("\bin\GL\SonixHidDll.dll")` e grava os ponteiros no objeto:
  `+0x60 FindDeviceWithID`, `+0x64 HIDReportOut` (SetFeature), `+0x68 HIDReportIn` (GetFeature), `+0x6c HIDReportIn2`, `+0x70 HIDReportIn3`, `+0x74 HIDReportOut2`.
- Campos do objeto: `+0x110 CommandDelayTime` (10 ms), `+0x10c MemoryType` (0 = EEPROM), `+0xf8` mutex, `+0x118` tabela de frequência (16 valores), `+0x128` tabela de brilho (16 valores), `+0x15c/+0x160/+0x164` `DevRGBChannelR/G/B`.
- Atraso: `FUN_10006540(ms)` é espera ativa com `QueryPerformanceCounter`.
- Vtable da classe Writer em `0x1014e93c` (Evidence `ev_3fa24ea9…`), métodos usados abaixo pelo deslocamento.

## Formato comum dos comandos (observado)

Todo comando é um **Feature report de 64 bytes** (`HIDReportOut`), byte 0 = `0x04`, byte 1 = opcode, restante zero salvo indicação.
Após cada envio, o Writer espera `CommandDelayTime` e lê um Feature report (`HIDReportIn`); **ack = `resp[3] == 0x01`**. Tenta até 5 vezes
(Evidence `ev_b28b149c…` assembly 0x10003489–0x100034a2, `ev_2568ae03…` 0x10006aa9–0x10006ac2).

Leitura de resultado (`FUN_10003dc0`, `ev_7be84500…`): envia `[04 02 00…]`, espera, lê Feature; com ack, devolve `u32 LE` em `resp[4..8]`.
É usada após escritas como verificação (soma dos bytes enviados).

| Opcode | Função (vtable) | Layout enviado | Resposta | Evidence |
|---|---|---|---|---|
| `0x02` | `FUN_10003dc0` (interno) | `04 02` | ack + `u32` em `resp[4..8]` | `ev_7be84500…` |
| `0x05` | `FUN_10003f10` (via `+0x24` Connect) | `04 05 00 00 00 00 00 00 02`; depois lê 2×64 B para `this+0x78` e confere soma com `0x02` | 128 B de dados do dispositivo | `ev_7be84500…`, consts `ev_99561875…` |
| `0x18` / `0x19` | `FUN_10003ae0(bool)` (`+0x58`) | `04 18` se `true`, `04 19` se `false` | ack (2 tentativas) | `ev_7be84500…`, consts `ev_99561875…` |
| `0xab` | `FUN_100032f0` (`+0x54`, “Verify”) | `04 ab`; com ack, envia 64 B aleatórios crus; depois `0x02` e compara com `f(r)` | ver abaixo | `ev_b28b149c…` |
| `0x81 0x83 0x85 0x91 0x93` / `0xb1` | `FUN_10006140` (escrita de bloco) via `+0x38` `FUN_100041d0(grupo)` e `+0x3c` `FUN_10004300` (`0xb1`) | cabeçalho `04 op 00 00 00 00 00 00 n_lo n_hi` (n = nº de blocos de 64 B); com ack, cada bloco de 64 B é enviado cru; no fim `0x02` | ack; `0x02` = soma | `ev_ed2b1eff…`, tabela `ev_0a9f05a4…` |
| `0x80 0x82 0x84 0x90 0x92` | `FUN_100065b0(.., read=1)` via `+0x40` | `04 op 00 00 00 00 00 00 01` | ack; depois lê 64 B do registro + soma via `0x02` | `ev_9b0d06c4…` |
| `0x60 0x62 0x64 0x70 0x72` | `FUN_100065b0(.., read=0)` via `+0x44` (lê registro de um modo específico; `resp` byte 3 = modo de fio) | `04 op 00 <modo> 00 00 00 00 01` | idem | `ev_9b0d06c4…` |
| `0xd1 0xd3 0xd5` | `FUN_10006930` (`+0x6c`, LED count dos headers ARGB1..3) | 1º: `04 op 00 00 00 00 00 00 01`; com ack, 2º: `count op 00 00 00 00 00 00 01 00… aa 55` (`[0]=count`, `[0x3e..0x3f]=aa 55`); depois `0x02` | ack | `ev_2568ae03…` |
| `0xd2 0xd4 0xd6` | `FUN_10006750` (leitura de LED count) | `04 op 00 00 00 00 00 00 01` | ack + 64 B | `ev_9b0d06c4…` |
| `0xa1` | `FUN_10006b60` (`+0x60`) | `04 a1 00 00 00 00 00 00 01` | ack, depois `0x02` | `ev_9b0d06c4…` |
| `0xb2` | `FUN_10007250(bool)` (`+0x70`, escrita do Global Control) | `04 b2 00 00 00 00 00 00 01`; com ack reenvia com `[0] = enabled` | ack, depois `0x02` | `ev_d6e4ff69…` |
| `0xb3` | `FUN_100074e0` (`+0x74`, leitura do Global Control) | `04 b3 00 00 00 00 00 00 01` | ack; lê 64 B, `enabled = resp[0] == 1` | `ev_d6e4ff69…` |
| `0xc1` | `FUN_100036b0` (`+0x64`) | `04 c1` | ack, depois `0x02` | `ev_e90c70b1…` |
| `0xe0` | `FUN_100031b0` (`+0x28`) | `04 e0` | ack (3 tentativas, `Sleep(100)`) | `ev_e90c70b1…` |
| `0xf0` / `0xf1` | `FUN_10003a70` (`+0x34`) / `FUN_10003a00` (`+0x30`) | `04 f0` / `04 f1` | **sem leitura** | `ev_e90c70b1…`, `ev_7be84500…` |
| `0xf1 v` / `0xf3 v` | `FUN_10006cd0` (`+0x48`) / `FUN_10006e70` (`+0x4c`) | `04 f1 v` / `04 f3 v`, `v ∈ {1,2,4,7}` para arg 0..3 | ack | `ev_d6e4ff69…` |
| Interrupt OUT `0x81` | `FUN_10007010` (`+0x50`, Music Rhythm) | 3× `81 seq rgb…` via `HIDReportOut2` | — | `ev_d6e4ff69…` (fora do escopo) |
| Interrupt IN | `FUN_100038e0` (`+0x68`, temperaturas) | lê 64 B, `T = b[2]+b[3]/100` etc. | — | `ev_e90c70b1…` (fora do escopo) |

### Verificação `0xab` (anti-clone, host verifica o dispositivo)

`f(r) = ((((r[0x0c]+r[0x03]) & 0xff) << 24) | (((r[0x3f]*8) & 0xff) << 16) | (((r[0x06]-r[0x0d]) & 0xff) << 8) | (r[0x24] & 0xa5)) ^ 0x54711688`
(assembly 0x1000339f–0x100033ea, `ev_b28b149c…`). A escrita de grupo (`FUN_100041d0`) só prossegue se `+0x24` (Connect) e `+0x54` (Verify) retornarem verdadeiro.

## Registro de iluminação (64 B por grupo) — montado por `FUN_10004820` e handlers de modo

Tabela de modos em `0x1016f5f0` (Evidence `ev_629739fa…`): índice de UI → handler. Handlers: `ev_449b0d52…`, `ev_114eb32a…`.

| Offset | Conteúdo | Fonte |
|---|---|---|
| `0x00` | modo de fio: Breathing `0x01`, Spectrum `0x03`, Static `0x04`, Wave `0x05`, Marquee `0x10`, HeartBeat `0x07`, Off `0x00` | handlers + `FUN_10004650` |
| `0x01..0x03` | Cor 1 na ordem `DevRGBChannelR/G/B` (padrão R, G, B) | handlers |
| `0x05..0x07` | Cor 2 (se `EnableColor2 == 0`, cópia da Cor 1) | Breathing/Marquee/HeartBeat |
| `0x37` | 8º parâmetro do perfil (só Wave grava) | Wave |
| `0x39` | `IsRandom == 1` | Breathing/HeartBeat |
| `0x3a` | **valor** da tabela de brilho (`LightBrightnessLevel[nível]`) | todos |
| `0x3b` | **valor** da tabela de frequência (`LightFrequencyLevel[nível]`) | todos |
| `0x3c` | direção | Wave/Marquee |
| `0x3e..0x3f` | `aa 55` | todos |

Brilho e velocidade vão no fio como **valor da tabela**, não como índice (a leitura em `FUN_10004440` converte de volta procurando o valor no vetor).

Mapeamento índice de UI → modo de fio (`FUN_10004650`, `ev_15877e9b…`): 0→1, 1→3, 2→4, 3→5, 4→0x10, 5→7, 6→0.

## Grupos e LED count (UI — `Bin/GL/LightingAdvUI.xml`, Main.dll `static-main.json`)

- Grupos visíveis: 5 — `ARGB1`, `ARGB2`, `ARGB3`, `RGB1`, `RGB2` (`ID_LIGHT_Group_1..5`). Grupos 6–9 (`RGB6`, `PWM1..3`) estão comentados no XML.
  Corresponde aos 5 opcodes de escrita `0x81 0x83 0x85 0x91 0x93` (tabela em `0x1016f5b0`). `0xb1` = escrita com grupo `0xff` (`FUN_10004300`).
- LED count: só ARGB1..3; `NumberEdit MinValue="1" MaxValue="60"` (padrão 30); controles com `Visible="0"` nesta build.

## `SafeClose.exe` (`static-safeclose.json`, Evidence `ev_44d1a970…`)

Não fala com o dispositivo: `RegisterWindowMessageW` + `PostMessageW(HWND_BROADCAST)` e espera os processos `GamingForceZ.exe`/`GamingForce.exe` saírem
(`CreateToolhelp32Snapshot`, `Sleep(1000)`). Não importa `hid.dll`.

## Resultado da captura dinâmica (Fase 3)

| Pergunta | Resposta | Captura |
|---|---|---|
| Ação da UI → comando | Global Control → `0xb2 en`; Restore → LED count + registro atual; Close/SafeClose → `0x18`/`0x19`; Colorful → nada | `D-off-global-on-g3`, `24-…`, `25-close`, `26-safeclose` |
| Direção `0x3c` | `0` esquerda, `1` direita (Onda) | `C-modes-g3` |
| Ordem RGB, brilho, velocidade, modos | confirmados | `04-05-…`, `B-…`, `B2-…`, `C-…`, `C2-…` |
| `0xab` antes de escrever | o app sempre envia; o `soyo-argb` replica | todas as escritas |
| `0xa1`, `0xe0`, `0xf0`, byte `0x37` | nunca observados / sempre 0 | — |
| `0xc1`, `0xf1`, `0xf3`, `0x18`, `0x19` | aparecem, mas não precedem as escritas por slider | `01-startup`, `B-…` |

Especificação consolidada: `docs/protocol.md`.
