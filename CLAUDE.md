# Cardeal — instruções para assistentes de IA

Leia primeiro `docs/19-estado-e-processo.md` (estado real do projeto e o que não repetir).

## Regra de UI (inegociável) — ADR-0015

**Telas só compõem componentes de `cardeal-ui`; nunca constroem com `egui` cru.**

- Em `crates/cardeal-desktop/src` (`tela_*.rs`, `main.rs`): proibido `egui::Frame`, `egui::Stroke`,
  `Color32`, `ui.painter`, `ui.separator`, `ui.checkbox`, `egui::ComboBox`, `TextEdit`, `RichText`,
  `Window`, `Grid`, `TableBuilder`, `ui.button`, `ui.label`, `ui.heading`, `ui.add_sized`.
  Layout (`ui.horizontal`, `ui.columns`, `Espaco::…`) é permitido.
- Falta o componente? **Crie em `cardeal-ui` primeiro** (atom → molecule → organism, com entrada em
  `examples/galeria.rs`) e só depois use. Nunca "só dessa vez".
- Procure antes de criar: inventário em `docs/12-ui-ux.md` §7.1 (ex.: `dialogo_confirmacao` já existe).
- Só tokens: `ui.cores()`, `Espaco`, `Raio`, `Rotulo`. Nenhuma cor/tamanho literal numa tela.
- Antes de commitar UI: `cargo xtask verificar-ui` (catraca: `xtask/ui-baseline.toml` só pode
  descer). Exceção rara: `// ui-livre: <motivo>`.
