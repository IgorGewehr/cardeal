//! O fluxo de tempo real no desktop: uma thread que fica conectada, reconecta sozinha (com
//! recuo) continuando do último `id`, e entrega [`AvisoTempoReal`]s por canal — a tela drena
//! a cada quadro, e `ao_chegar` (o `request_repaint`) acorda a interface parada.

use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use cardeal_kernel::Resultado;
use cardeal_protocol::rota_eventos;

use super::tempo_real::{AvisoTempoReal, LeitorSse};
use super::{sem_empresa, Fluxo, Remoto, Transporte};

/// Recuo máximo entre reconexões.
const RECUO_MAXIMO: Duration = Duration::from_secs(30);

/// Um acompanhamento em curso; para ao ser solto (em até um batimento do servidor).
pub struct Assinatura {
    avisos: mpsc::Receiver<AvisoTempoReal>,
    parar: Arc<AtomicBool>,
}

impl Assinatura {
    /// Os avisos que chegaram desde a última chamada.
    pub fn drenar(&self) -> Vec<AvisoTempoReal> {
        self.avisos.try_iter().collect()
    }
}

impl Drop for Assinatura {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
    }
}

impl Remoto {
    /// Passa a acompanhar as alterações da empresa escolhida.
    ///
    /// # Errors
    /// Nenhuma empresa escolhida.
    pub fn acompanhar(&self, ao_chegar: impl Fn() + Send + 'static) -> Resultado<Assinatura> {
        let empresa = self.empresa.ok_or_else(sem_empresa)?;
        let (tx, avisos) = mpsc::channel();
        let parar = Arc::new(AtomicBool::new(false));
        let laco = Laco {
            transporte: Arc::clone(&self.transporte),
            caminho: rota_eventos(empresa),
            token: self.token.clone(),
            tx,
            parar: Arc::clone(&parar),
            ao_chegar: Box::new(ao_chegar),
        };
        std::thread::Builder::new()
            .name("cardeal-tempo-real".into())
            .spawn(move || laco.rodar())
            .map_err(|e| {
                cardeal_kernel::Erro::novo(cardeal_kernel::CodigoErro::FALHA_INTERNA, e.to_string())
            })?;
        Ok(Assinatura { avisos, parar })
    }
}

struct Laco {
    transporte: Arc<dyn Transporte>,
    caminho: String,
    token: String,
    tx: mpsc::Sender<AvisoTempoReal>,
    parar: Arc<AtomicBool>,
    ao_chegar: Box<dyn Fn() + Send>,
}

enum Fim {
    /// Parar de vez (soltaram a assinatura, ou a sessão caiu).
    Encerrar,
    /// A conexão caiu: reconectar.
    Reconectar,
}

impl Laco {
    fn parado(&self) -> bool {
        self.parar.load(Ordering::Relaxed)
    }

    /// Entrega um aviso; `false` = parar (ninguém mais ouve, ou a sessão caiu).
    fn entregar(&self, aviso: AvisoTempoReal) -> bool {
        let fim = aviso == AvisoTempoReal::SessaoEncerrada;
        if self.tx.send(aviso).is_err() {
            return false;
        }
        (self.ao_chegar)();
        !fim
    }

    fn rodar(self) {
        let mut ultimo = None;
        let mut recuo = Duration::from_secs(1);
        while !self.parado() {
            match self
                .transporte
                .abrir_fluxo(&self.caminho, Some(&self.token), ultimo)
            {
                Ok(Fluxo::Aberto(corpo)) => {
                    recuo = Duration::from_secs(1);
                    let mut leitor = LeitorSse::continuando(ultimo);
                    let fim = self.ler(corpo, &mut leitor);
                    ultimo = leitor.ultimo_id();
                    if matches!(fim, Fim::Encerrar) {
                        return;
                    }
                }
                Ok(Fluxo::Recusado(r)) if r.status == 401 || r.status == 403 => {
                    let _ = self.entregar(AvisoTempoReal::SessaoEncerrada);
                    return;
                }
                Ok(Fluxo::Recusado(r)) => {
                    tracing::warn!(status = r.status, "tempo real recusado; tentando de novo");
                }
                Err(e) => tracing::debug!(erro = %e, "tempo real sem conexão; tentando de novo"),
            }
            self.dormir(recuo);
            recuo = (recuo * 2).min(RECUO_MAXIMO);
        }
    }

    fn ler(&self, mut corpo: Box<dyn std::io::Read + Send>, leitor: &mut LeitorSse) -> Fim {
        let mut buf = [0u8; 4096];
        loop {
            // O servidor manda batimento a cada 25 s: a leitura sempre volta para conferir.
            if self.parado() {
                return Fim::Encerrar;
            }
            match corpo.read(&mut buf) {
                Ok(0) | Err(_) => return Fim::Reconectar,
                Ok(n) => {
                    for aviso in leitor.alimentar(&buf[..n]) {
                        if !self.entregar(aviso) {
                            return Fim::Encerrar;
                        }
                    }
                }
            }
        }
    }

    /// Dorme em fatias, para soltar a assinatura não esperar o recuo inteiro.
    fn dormir(&self, total: Duration) {
        let fatia = Duration::from_millis(200);
        let mut dormido = Duration::ZERO;
        while dormido < total && !self.parado() {
            std::thread::sleep(fatia);
            dormido += fatia;
        }
    }
}
