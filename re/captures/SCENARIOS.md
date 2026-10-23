# Cenários de captura (usbmon1, filtro `usb.bus_id == 1 && usb.device_address == 5`)

| NN | Ação exata | Horário |
|---|---|---|
| 01-startup | iniciar o app (dumpcap já rodando antes do wine), até a UI aparecer; abriu sem erro | 2026-10-23T17:44:33 |
| 02-idle | nenhuma ação por 10 s (app aberto) | 2026-10-23T17:45:23 |
| 22-global-on-static-red | (pedido como 03) ARGB1, Static, #FF0000, brilho máximo — Global Control estava LIGADO, então o app gravou com 0xb1 (todos os grupos) | 2026-10-23T17:48:03 |
| 23-global-off | clicar em Global Control para desligar (nada mais); LEDs estavam todos vermelhos após 22 | 2026-10-23T17:50:20 |
| 03-static-red-g1 | Global Control desligado; ARGB1, Static, #FF0000, brilho arrastado até o máximo | 2026-10-23T17:51:26 |
| 23b-global-off-click-and-g1-edits | usuário clicou em Global Control (desligou: 0xb2 enabled=0) e mexeu em cor/brilho; app gravou no grupo 0x81 (ARGB1); LEDs físicos não mudaram | 2026-10-23T17:53:47 |
| 06-group-select-g2-g3 | selecionou ARGB2 (app grava 0x83) e depois ARGB3 (0x85), Static vermelho, mexeu no brilho; LEDs físicos estão no conector ARGB3 | 2026-10-23T17:55:13 |
| A-static-g3 | ARGB3 Static: brilho máx; paleta #ff0000, #00ff00, #0000ff, #ff0000; brilho nível 1; brilho máx | 2026-10-23T17:56:21 |
| 24-restore-and-static-red-g3 | (era a captura A) usuário clicou Restore no meio; app gravou LED count ARGB3=40 (0xd5) e Static vermelho no ARGB3; verde/azul não foram feitos | 2026-10-23T17:57:36 |
| 04-05-static-green-blue-g3 | ARGB3 Static brilho máx: branco (estado anterior), #00ff00, #0000ff | 2026-10-23T17:58:24 |
| B-breathing-g3 | ARGB3 Breathing: vermelho, Color2 desmarcado; velocidade mín; brilho máx; velocidade máx (passos 4-5 não chegaram) | 2026-10-23T17:59:33 |
| B2-breathing-color2-random-g3 | ARGB3 Breathing vermelho: Enable Color2 + verde; Random on; Random off | 2026-10-23T18:00:49 |
| C-modes-g3 | ARGB3: Spectrum; Wave esquerda; Wave direita; (colorful?); brilho máx — Marquee/HeartBeat/Close não chegaram | 2026-10-23T18:01:54 |
| C2-marquee-heartbeat-off-g3 | ARGB3: Marquee (valores padrão do perfil), HeartBeat (padrão), depois voltou a Spectrum; Close não chegou | 2026-10-23T18:03:21 |
| D-off-global-on-g3 | ARGB3 Close (registro 64 B zerado); Global Control ligado (0xb2 01 + escrita 0xb1) e desligado de novo; Static azul/vermelho no ARGB3 | 2026-10-23T18:04:35 |
| 25-close | fechar o app pelo X; LEDs não mudaram | 2026-10-23T18:05:24 |
| 26-safeclose | com o app aberto (em segundo plano após o X), rodar `wine SafeClose.exe` | 2026-10-23T18:05:45 |
| V1-soyo-argb-static-green-g3 | soyo-argb set --zone 3 --mode static --color 00ff00 --brightness 15 (app Wine fechado) | 2026-10-23T18:25:26 |
| V2-soyo-argb-modes-g3 | soyo-argb: breathing, wave left/right, marquee, heartbeat, off, static | 2026-10-23T18:25:26 |
| V3-soyo-argb-ledcount-restore-global | soyo-argb led-count 3 40; restore; set --zone all static ff0000 brilho 5 | 2026-10-23T18:25:26 |
| V4-gui-wave-g3 | soyo-argb-gui: zona 3, Onda, Aplicar | 2026-10-23T18:25:26 |
| V5-gui-auto-apply | GUI com aplicação automática: mudanças no ARGB1 gravadas sozinhas (0x81) + clique em reenviar tudo | 2026-10-23T18:50:15 |
| V6-gui-auto-apply-g3 | GUI com aplicação automática: ARGB3, Respiração e vermelho, sem clicar em aplicar | 2026-10-23T18:50:15 |
