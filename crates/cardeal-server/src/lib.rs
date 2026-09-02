//! O motor do Cardeal, como biblioteca — usado pelo modo monoposto (`cardeal-desktop`
//! com a feature `motor-embutido`), que sobe o motor como thread do próprio processo em
//! vez de um binário separado. Ver `docs/01-arquitetura-geral.md` §6 e
//! `docs/adr/0009-transporte-in-process.md`.
//!
//! Implementação pendente — ver `docs/19-estado-e-processo.md` e `docs/17-roadmap.md`.
#![forbid(unsafe_code)]
