//! Tempo real: quem está olhando uma empresa fica sabendo, em milissegundos, que algo mudou.
//!
//! O **outbox é a verdade** e o sinal é só um despertador. Um comando bem-sucedido grava a
//! alteração no outbox na mesma transação (então nada confirmado deixa de virar evento) e,
//! depois de responder, toca o sinal da empresa. Cada conexão acordada lê do outbox o que
//! veio depois do último `seq` que entregou. Sinal perdido, rajada de comandos, conexão que
//! caiu e voltou: tudo se resolve relendo do ponto certo.
//!
//! Custo: uma empresa sem ninguém olhando não tem canal (tocar o sinal é um lookup num mapa);
//! uma conexão parada é uma tarefa tokio esperando — alguns KB — e **não** segura a empresa
//! aberta: o batimento revalida só a sessão de conta, sem tocar a frota.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cardeal_kernel::Id;
use parking_lot::Mutex;
use tokio::sync::watch;

/// Os canais por empresa e a contagem de conexões.
pub struct TempoReal {
    canais: Mutex<HashMap<Id, watch::Sender<u64>>>,
    conexoes: Arc<AtomicUsize>,
    teto: usize,
}

/// Uma vaga de conexão; devolvida ao cair.
pub struct Vaga(Arc<AtomicUsize>);

impl Drop for Vaga {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl TempoReal {
    /// Um registro vazio que aceita até `teto` conexões simultâneas.
    #[must_use]
    pub fn novo(teto: usize) -> Self {
        Self {
            canais: Mutex::new(HashMap::new()),
            conexoes: Arc::new(AtomicUsize::new(0)),
            teto: teto.max(1),
        }
    }

    /// Algo mudou na `empresa`: acorda quem a acompanha (ninguém olhando = nada a fazer).
    pub fn avisar(&self, empresa: Id) {
        if let Some(tx) = self.canais.lock().get(&empresa) {
            tx.send_modify(|n| *n = n.wrapping_add(1));
        }
    }

    /// Passa a acompanhar a `empresa`.
    pub fn assinar(&self, empresa: Id) -> watch::Receiver<u64> {
        self.canais
            .lock()
            .entry(empresa)
            .or_insert_with(|| watch::channel(0).0)
            .subscribe()
    }

    /// Reserva uma vaga de conexão, ou `None` acima do teto.
    pub fn reservar(&self) -> Option<Vaga> {
        let antes = self.conexoes.fetch_add(1, Ordering::Relaxed);
        let vaga = Vaga(Arc::clone(&self.conexoes));
        (antes < self.teto).then_some(vaga)
    }

    /// Conexões abertas agora.
    pub fn conexoes(&self) -> usize {
        self.conexoes.load(Ordering::Relaxed)
    }

    /// Esquece os canais de empresas que ninguém mais acompanha (manutenção).
    pub fn podar(&self) {
        self.canais.lock().retain(|_, tx| tx.receiver_count() > 0);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[tokio::test]
    async fn aviso_acorda_quem_assina_e_poda_esquece_canal_sem_ninguem() {
        let t = TempoReal::novo(10);
        let e = Id::novo();
        t.avisar(e); // sem assinante: não cria canal
        assert!(t.canais.lock().is_empty());
        let mut rx = t.assinar(e);
        t.avisar(e);
        t.avisar(e);
        rx.changed().await.unwrap(); // rajada vira um despertar só
        assert!(!rx.has_changed().unwrap());
        drop(rx);
        t.podar();
        assert!(t.canais.lock().is_empty());
    }

    #[test]
    fn teto_de_conexoes() {
        let t = TempoReal::novo(2);
        let a = t.reservar().unwrap();
        let _b = t.reservar().unwrap();
        assert!(t.reservar().is_none());
        assert_eq!(t.conexoes(), 2);
        drop(a);
        assert!(t.reservar().is_some());
    }
}
