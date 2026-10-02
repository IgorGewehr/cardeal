//! A frota: as empresas abertas neste processo. É aqui que mora a economia de memória da
//! ADR-0016 — uma empresa só ocupa RAM enquanto está sendo usada.
//!
//! - **Abre sob demanda.** A primeira requisição de uma empresa abre a base (perfil
//!   [`cardeal_storage::ConfigArmazenamento::servidor`], com réplica contínua); as seguintes reaproveitam.
//! - **Despeja ociosas.** [`Frota::despejar_ociosas`] fecha as que passaram da ociosidade.
//! - **Teto.** Acima de `teto` abertas, a menos usada (e livre) é despejada antes de abrir outra.
//!
//! Nunca duas aberturas da mesma base: cada empresa tem uma [`Vaga`] com o próprio `Mutex`, e
//! uma vaga só sai do mapa quando ninguém além do mapa a segura (`Arc::strong_count == 1`,
//! conferido sob o lock do mapa — ninguém consegue um clone novo nesse meio-tempo).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::ConfigServidor;
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use cardeal_motor::{MotorLocal, Plano, SessaoLocal};
use parking_lot::Mutex;

/// Por quanto tempo a sessão de um usuário numa empresa (papéis × permissões) é reaproveitada
/// antes de ser remontada — uma mudança de papel vale em até este tempo.
const VALIDADE_SESSAO_EMPRESA: Duration = Duration::from_secs(60);

/// Uma empresa aberta: o motor e as sessões já montadas dos usuários dela.
pub struct EmpresaAberta {
    motor: MotorLocal,
    sessoes: Mutex<HashMap<Id, (Arc<SessaoLocal>, Instant)>>,
}

impl EmpresaAberta {
    /// O motor da empresa.
    #[must_use]
    pub const fn motor(&self) -> &MotorLocal {
        &self.motor
    }

    /// A sessão do `usuario`, aberta pela sessão de conta `sessao` (que vira o "dispositivo"
    /// nas auditorias). Reaproveitada por [`VALIDADE_SESSAO_EMPRESA`].
    ///
    /// # Errors
    /// `SESSAO_INVALIDA` se o usuário não existe ou foi desativado na empresa.
    pub fn sessao(&self, sessao: Id, usuario: Id) -> Resultado<Arc<SessaoLocal>> {
        if let Some((s, quando)) = self.sessoes.lock().get(&sessao) {
            if quando.elapsed() < VALIDADE_SESSAO_EMPRESA && s.usuario() == usuario {
                return Ok(Arc::clone(s));
            }
        }
        let nova = Arc::new(self.motor.sessao_do_usuario(usuario, sessao)?);
        self.sessoes
            .lock()
            .insert(sessao, (Arc::clone(&nova), Instant::now()));
        Ok(nova)
    }
}

impl EmpresaAberta {
    /// Esquece as sessões montadas de um usuário (desativado: o acesso cai já, não em 60 s).
    pub fn esquecer_usuario(&self, usuario: Id) {
        self.sessoes
            .lock()
            .retain(|_, (s, _)| s.usuario() != usuario);
    }
}

struct Vaga {
    aberta: Mutex<Option<Arc<EmpresaAberta>>>,
    /// Milissegundos desde o início da frota — barato de tocar sem lock.
    ultimo_uso: AtomicU64,
}

/// As empresas abertas.
pub struct Frota {
    plano: Arc<Plano>,
    config: ConfigServidor,
    ociosidade: Duration,
    teto: usize,
    inicio: Instant,
    vagas: Mutex<HashMap<Id, Arc<Vaga>>>,
}

impl Frota {
    /// Uma frota vazia sobre a pasta das bases. Todas as empresas compartilham o `plano`.
    #[must_use]
    pub fn nova(plano: Arc<Plano>, config: ConfigServidor) -> Self {
        Self {
            plano,
            ociosidade: config.ociosidade,
            teto: config.teto_empresas.max(1),
            config,
            inicio: Instant::now(),
            vagas: Mutex::new(HashMap::new()),
        }
    }

    fn agora_ms(&self) -> u64 {
        u64::try_from(self.inicio.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// A empresa aberta — abrindo agora, se preciso. **Bloqueante** (abre arquivo, migra): chame
    /// de dentro de `spawn_blocking`.
    ///
    /// # Errors
    /// `NAO_ENCONTRADO` se a empresa não tem base neste servidor; erro de abertura/migração.
    pub fn obter(&self, empresa: Id) -> Resultado<Arc<EmpresaAberta>> {
        let mut despejadas = Vec::new();
        let vaga = {
            let mut vagas = self.vagas.lock();
            let vaga = Arc::clone(vagas.entry(empresa).or_insert_with(|| {
                Arc::new(Vaga {
                    aberta: Mutex::new(None),
                    ultimo_uso: AtomicU64::new(0),
                })
            }));
            if vagas.len() > self.teto {
                despejadas = self.despejar_menos_usadas(&mut vagas, empresa);
            }
            vaga
        };
        // Fechar uma base espera a thread do escritor terminar: fora de qualquer lock.
        drop(despejadas);

        vaga.ultimo_uso.store(self.agora_ms(), Ordering::Relaxed);
        let mut aberta = vaga.aberta.lock();
        if let Some(e) = aberta.as_ref() {
            return Ok(Arc::clone(e));
        }
        let e = Arc::new(self.abrir(empresa)?);
        *aberta = Some(Arc::clone(&e));
        Ok(e)
    }

    fn abrir(&self, empresa: Id) -> Resultado<EmpresaAberta> {
        let caminho = self.config.caminho_empresa(empresa);
        // Nunca cria arquivo para um id desconhecido: só o provisionamento cria empresas.
        if !caminho.is_file() {
            return Err(Erro::novo(
                CodigoErro::NAO_ENCONTRADO,
                format!("a empresa {empresa} não existe neste servidor"),
            ));
        }
        let inicio = Instant::now();
        let motor =
            MotorLocal::abrir_com_plano(self.config.armazenamento_empresa(empresa), &self.plano)?;
        if motor.empresa() != empresa {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!(
                    "a base de {empresa} pertence a outra empresa ({})",
                    motor.empresa()
                ),
            ));
        }
        tracing::info!(%empresa, ms = inicio.elapsed().as_millis(), "empresa aberta");
        Ok(EmpresaAberta {
            motor,
            sessoes: Mutex::new(HashMap::new()),
        })
    }

    /// Quantas empresas estão abertas agora.
    #[must_use]
    pub fn abertas(&self) -> usize {
        self.vagas
            .lock()
            .values()
            .filter(|v| v.aberta.lock().is_some())
            .count()
    }

    /// Fecha as empresas sem uso há mais que a ociosidade e sem requisição em curso. Antes de
    /// fechar, poda as respostas idempotentes gravadas antes de `idempotencia_antes_de` — a
    /// faxina acontece aqui, na manutenção, nunca no caminho de uma requisição. Devolve quantas
    /// fechou. **Bloqueante** (espera as threads de escrita): `spawn_blocking`.
    pub fn despejar_ociosas(&self, idempotencia_antes_de: Instante) -> usize {
        let limite = self
            .agora_ms()
            .saturating_sub(u64::try_from(self.ociosidade.as_millis()).unwrap_or(u64::MAX));
        let despejadas: Vec<_> = {
            let mut vagas = self.vagas.lock();
            let alvo: Vec<Id> = vagas
                .iter()
                .filter(|(_, v)| v.ultimo_uso.load(Ordering::Relaxed) <= limite)
                .map(|(id, _)| *id)
                .collect();
            alvo.into_iter()
                .filter_map(|id| retirar_se_livre(&mut vagas, id))
                .collect()
        };
        let n = despejadas.iter().filter(|r| r.0.is_some()).count();
        for e in despejadas.iter().filter_map(|r| r.0.as_ref()) {
            if let Err(erro) = e.motor.podar_idempotencia(idempotencia_antes_de) {
                tracing::warn!(empresa = %e.motor.empresa(), erro = %erro.mensagem, "poda de idempotência falhou");
            }
        }
        drop(despejadas);
        if n > 0 {
            tracing::info!(fechadas = n, "empresas ociosas fechadas");
        }
        n
    }

    /// Fecha todas as empresas, esperando as requisições em curso terminarem (desligamento).
    /// Devolve quantas fechou. **Bloqueante.**
    pub fn fechar_todas(&self) -> usize {
        let vagas: Vec<Arc<Vaga>> = self.vagas.lock().drain().map(|(_, v)| v).collect();
        vagas.iter().filter_map(|v| v.aberta.lock().take()).count()
    }

    /// Acima do teto: retira as vagas livres menos usadas até caber, nunca a que está sendo
    /// pedida agora.
    fn despejar_menos_usadas(
        &self,
        vagas: &mut HashMap<Id, Arc<Vaga>>,
        pedida: Id,
    ) -> Vec<Retirada> {
        let mut candidatas: Vec<(u64, Id)> = vagas
            .iter()
            .filter(|(id, _)| **id != pedida)
            .map(|(id, v)| (v.ultimo_uso.load(Ordering::Relaxed), *id))
            .collect();
        candidatas.sort_unstable();
        let mut saida = Vec::new();
        for (_, id) in candidatas {
            if vagas.len() <= self.teto {
                break;
            }
            if let Some(e) = retirar_se_livre(vagas, id) {
                saida.push(e);
            }
        }
        saida
    }
}

/// Uma vaga retirada do mapa, com a empresa que estava aberta nela (se estava) — para ser
/// derrubada **fora** do lock, porque fechar a base espera a thread do escritor.
struct Retirada(Option<Arc<EmpresaAberta>>);

/// Retira a vaga do mapa se ninguém além do mapa a segura e nenhuma requisição usa a empresa.
/// `None` = ocupada, fica para a próxima rodada.
fn retirar_se_livre(vagas: &mut HashMap<Id, Arc<Vaga>>, id: Id) -> Option<Retirada> {
    let vaga = vagas.get(&id)?;
    if Arc::strong_count(vaga) > 1 {
        return None;
    }
    let mut aberta = vaga.aberta.try_lock()?;
    if aberta.as_ref().is_some_and(|e| Arc::strong_count(e) > 1) {
        return None;
    }
    let conteudo = aberta.take();
    drop(aberta);
    vagas.remove(&id);
    Some(Retirada(conteudo))
}
