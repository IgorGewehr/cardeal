//! Abre uma ordem de serviço.
//!
//! `docs/modulos/os.md` §5. Na abertura, o nome do cliente, `equipamento` (o aparelho
//! trazido para reparo) e `defeito_relatado` são obrigatórios — `equipamento` ainda pode ser
//! corrigido depois via `EditarDadosDaOrdem`. O cliente precisa já existir em
//! `clientes_pessoa` (verificado aqui via `mod_clientes::pessoa_por_id`, a mesma transação).
//! O vínculo com `agenda.CriarCompromisso` fica para quando o módulo `agenda` existir — por
//! ora `compromisso` não é gravado.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_modkit::{Comando, Ctx};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use mod_clientes::{adicionar_contato_comum, criar_pessoa_comum, pessoa_por_id, AdicionarContato};
use mod_clientes::{ContatoInicial, CriarPessoa};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::erros::ErroOs;
#[cfg(feature = "sqlite")]
use crate::eventos::OrdemAberta;
use crate::ordem::FichaEntrada;
#[cfg(feature = "sqlite")]
use crate::ordem::OrdemServico;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioOs;

/// Abre uma ordem de serviço.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbrirOrdemServico {
    /// O cliente (papel `Cliente` em `clientes_pessoa`).
    pub cliente: Id,
    /// Descrição livre do aparelho trazido para reparo — **obrigatório**.
    pub equipamento: String,
    /// O que o cliente relatou querer resolver — **obrigatório** (junto do cliente, é o
    /// único texto que a abertura exige).
    pub defeito_relatado: String,
    /// O técnico responsável.
    pub tecnico_responsavel: Id,
    /// Prazo de garantia em dias sobre as peças aplicadas (padrão sugerido: 90).
    pub garantia_dias: u16,
    /// Previsão de entrega, nº de série e acessórios — tudo opcional.
    pub ficha: FichaEntrada,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemServicoAberta {
    /// A ordem criada.
    pub ordem_servico: Id,
    /// O número sequencial atribuído.
    pub numero: u64,
}

#[cfg(feature = "sqlite")]
impl Comando for AbrirOrdemServico {
    type Saida = OrdemServicoAberta;
    const PERMISSAO: &'static str = "os.ordem.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        abrir_ordem_comum(self, ctx, uow)
    }
}

/// Cadastra o cliente **e** abre a OS na mesma transação — o balcão atendendo alguém que
/// nunca veio antes. Antes a tela disparava três comandos soltos (criar pessoa → adicionar
/// contato → abrir OS); se a abertura falhasse, sobrava um cliente órfão, e tentar de novo o
/// duplicava.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbrirOrdemComClienteNovo {
    /// O cadastro do cliente (papel inicial deve ser `Cliente`).
    pub cliente: CriarPessoa,
    /// Contatos além do principal que já vai em `cliente.contato` (ex.: o e-mail, quando o
    /// principal é o `WhatsApp`).
    pub contatos_extras: Vec<ContatoInicial>,
    /// Descrição livre do aparelho trazido para reparo.
    pub equipamento: String,
    /// O que o cliente relatou querer resolver.
    pub defeito_relatado: String,
    /// O técnico responsável.
    pub tecnico_responsavel: Id,
    /// Prazo de garantia em dias.
    pub garantia_dias: u16,
    /// Previsão de entrega, nº de série e acessórios — tudo opcional.
    pub ficha: FichaEntrada,
}

/// O que [`AbrirOrdemComClienteNovo`] devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemComClienteNovoAberta {
    /// O cliente cadastrado.
    pub cliente: Id,
    /// A ordem aberta.
    pub ordem: OrdemServicoAberta,
}

#[cfg(feature = "sqlite")]
impl Comando for AbrirOrdemComClienteNovo {
    type Saida = OrdemComClienteNovoAberta;
    const PERMISSAO: &'static str = "os.ordem.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // O comando tem uma só permissão declarada; a de cadastrar pessoa é conferida aqui,
        // para não virar uma porta dos fundos para quem só pode abrir OS.
        if !ctx.concede("clientes.pessoa.criar") {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "sem permissão para cadastrar cliente",
            ));
        }
        let cliente = criar_pessoa_comum(self.cliente, ctx, uow)?.pessoa;
        for c in self.contatos_extras {
            adicionar_contato_comum(
                &AdicionarContato {
                    pessoa: cliente,
                    tipo: c.tipo,
                    valor: c.valor,
                    principal: true,
                },
                ctx,
                uow,
            )?;
        }
        let ordem = abrir_ordem_comum(
            AbrirOrdemServico {
                cliente,
                equipamento: self.equipamento,
                defeito_relatado: self.defeito_relatado,
                tecnico_responsavel: self.tecnico_responsavel,
                garantia_dias: self.garantia_dias,
                ficha: self.ficha,
            },
            ctx,
            uow,
        )?;
        Ok(OrdemComClienteNovoAberta { cliente, ordem })
    }
}

/// O corpo de [`AbrirOrdemServico`], reaproveitado por [`AbrirOrdemComClienteNovo`].
#[cfg(feature = "sqlite")]
fn abrir_ordem_comum(
    dados: AbrirOrdemServico,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<OrdemServicoAberta> {
    // 1. Numerar.
    let numero = RepositorioOs::novo(uow).proximo_numero()?;

    // 2. Validar: o cliente precisa existir em `clientes_pessoa` (porta pública de
    // `mod_clientes`, nunca lendo a tabela direto — `docs/contratos-internos.md` §7
    // regra 2).
    if pessoa_por_id(uow.conexao(), dados.cliente)?.is_none() {
        return Err(Erro::de_dominio(&ErroOs::ClienteInexistente));
    }

    // 3. Validar (domínio puro).
    let mut os = OrdemServico::abrir(
        ctx.empresa,
        numero,
        dados.cliente,
        dados.equipamento,
        dados.defeito_relatado,
        dados.tecnico_responsavel,
        ctx.hoje(),
        dados.garantia_dias,
    )
    .map_err(|e| Erro::de_dominio(&e))?;
    os.ficha = dados.ficha.normalizada();

    // 4. Persistir.
    RepositorioOs::novo(uow).inserir_ordem(&os)?;

    // 5. Publicar.
    uow.publicar(OrdemAberta {
        ordem_servico: os.id,
        cliente: os.cliente,
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(OrdemServicoAberta {
        ordem_servico: os.id,
        numero: os.numero,
    })
}
