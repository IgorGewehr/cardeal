//! `Armazenamento` — a fachada que reúne o escritor, o pool de leitura e as migrações.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use cardeal_kernel::{Id, Versao};

use crate::conexao::{self, Alvo};
use crate::config::ConfigArmazenamento;
use crate::contexto::ContextoEscrita;
use crate::erros::ErroArmazenamento;
use crate::escritor::Escritor;
use crate::leitor::Leitor;
use crate::migracao::{self, ConjuntoMigracoes, RelatorioMigracao};
use crate::nucleo;

type Resultado<T> = Result<T, ErroArmazenamento>;

/// A base aberta: um escritor único, um pool de leitura, o esquema migrado.
pub struct Armazenamento {
    escritor: Escritor,
    leitor: Leitor,
    versao_global: Arc<AtomicU64>,
    handle: Option<JoinHandle<()>>,
}

impl Armazenamento {
    /// Abre (criando se preciso) a base, aplica as migrações do núcleo e sobe o escritor.
    ///
    /// # Errors
    /// [`ErroArmazenamento::Abertura`] se o arquivo não abre; erros de migração se o esquema
    /// do núcleo falha ou foi alterado.
    #[allow(clippy::needless_pass_by_value)] // toma posse da config, inclusive da chave cripto
    pub fn abrir(cfg: ConfigArmazenamento) -> Resultado<Self> {
        let alvo = Alvo::de(&cfg);
        let mut conn_escritor = conexao::abrir_escritor(&alvo)?;
        if !cfg.somente_leitura {
            migracao::aplicar(&mut conn_escritor, &[nucleo::conjunto()])?;
        }

        let versao_global = Arc::new(AtomicU64::new(0));
        let (escritor, handle) = Escritor::iniciar(
            conn_escritor,
            Arc::clone(&versao_global),
            cfg.janela_lote_ms,
            cfg.maximo_lote,
        );

        let n_leitores = cfg.leitores.max(1);
        let mut leitores = Vec::with_capacity(n_leitores);
        for _ in 0..n_leitores {
            leitores.push(conexao::abrir_leitor(&alvo)?);
        }

        Ok(Self {
            escritor,
            leitor: Leitor::novo(leitores),
            versao_global,
            handle: Some(handle),
        })
    }

    /// O escritor único — todo comando escreve por aqui.
    #[must_use]
    pub const fn escritor(&self) -> &Escritor {
        &self.escritor
    }

    /// O pool de leitura.
    #[must_use]
    pub const fn leitor(&self) -> &Leitor {
        &self.leitor
    }

    /// A versão global atual (número de lotes já confirmados).
    #[must_use]
    pub fn versao_atual(&self) -> Versao {
        Versao::nova(self.versao_global.load(Ordering::SeqCst))
    }

    /// Aplica os conjuntos de migração dos módulos, passando pelo escritor (serializado com
    /// as demais escritas).
    ///
    /// # Errors
    /// [`ErroArmazenamento::MigracaoAlterada`], [`ErroArmazenamento::MigracaoFalhou`],
    /// [`ErroArmazenamento::DependenciaDeMigracaoAusente`].
    pub fn migrar(&self, conjuntos: &[ConjuntoMigracoes]) -> Resultado<RelatorioMigracao> {
        // O núcleo já foi aplicado na abertura; incluí-lo aqui só resolve o `depende_de`
        // dos módulos (as migrações já registradas são puladas após conferir o hash).
        let mut todos = vec![nucleo::conjunto()];
        todos.extend_from_slice(conjuntos);
        let conjuntos = todos;
        let ctx = ContextoEscrita::novo(Id::NULO, Id::NULO, Id::NULO, Id::NULO);
        self.escritor
            .executar(ctx, move |uow| {
                migracao::aplicar_passos(uow.conexao(), &conjuntos)
            })
            .map(|c| c.valor)
    }

    /// Encerra o escritor de forma limpa e espera a thread terminar.
    ///
    /// # Errors
    /// [`ErroArmazenamento::Sqlite`] se a thread do escritor tiver entrado em pânico.
    pub fn fechar(mut self) -> Resultado<()> {
        self.escritor.encerrar();
        match self.handle.take() {
            Some(h) => h.join().map_err(|_| {
                ErroArmazenamento::Sqlite("a thread do escritor entrou em pânico".into())
            }),
            None => Ok(()),
        }
    }
}

impl Drop for Armazenamento {
    fn drop(&mut self) {
        self.escritor.encerrar();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
