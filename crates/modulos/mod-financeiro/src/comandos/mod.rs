//! Os comandos do financeiro (`docs/modulos/financeiro.md` §5). Um arquivo, um comando,
//! sempre nas cinco etapas de `docs/15-convencoes-codigo.md` §3: **carregar, validar, lançar
//! no razão, persistir, publicar**.
//!
//! `LancarTitulo`/`BaixarParcela` são declarados em pares por espécie, porque a permissão
//! difere (`financeiro.receber.criar` × `financeiro.pagar.criar`) e um `Comando` tem uma só
//! `PERMISSAO`. O corpo comum a cada par vive nos helpers `lancar_titulo_comum` /
//! `baixar_parcela_comum` deste módulo — a diferença entre receber e pagar é só a espécie,
//! os papéis de conta e a permissão.

mod abrir_caixa;
mod baixar_pagamento;
mod baixar_recebimento;
mod cadastrar_caixa;
mod criar_categoria;
mod criar_conta_bancaria;
mod criar_recorrencia;
mod estornar_baixa;
mod fechar_caixa;
mod lancar_titulo_a_pagar;
mod lancar_titulo_a_receber;
mod materializar_recorrencia;
mod registrar_sangria;
mod registrar_suprimento;
mod renegociar_titulo;

pub use abrir_caixa::{AbrirCaixa, CaixaFoiAberto};
pub use baixar_pagamento::{BaixarPagamento, PagamentoBaixado};
pub use baixar_recebimento::{BaixarRecebimento, RecebimentoBaixado};
pub use cadastrar_caixa::{CadastrarCaixa, CaixaCadastrado};
pub use criar_categoria::{CategoriaCriada, CriarCategoria};
pub use criar_conta_bancaria::{ContaBancariaCriada, CriarContaBancaria};
pub use criar_recorrencia::{CriarRecorrencia, RecorrenciaCriada};
pub use estornar_baixa::{BaixaFoiEstornada, EstornarBaixa};
pub use fechar_caixa::{CaixaFoiFechado, FecharCaixa};
pub use lancar_titulo_a_pagar::{LancarTituloAPagar, TituloAPagarLancado};
pub use lancar_titulo_a_receber::{LancarTituloAReceber, TituloAReceberLancado};
pub use materializar_recorrencia::materializar_recorrencias_pendentes;
pub use registrar_sangria::{RegistrarSangria, SangriaFoiRegistrada};
pub use registrar_suprimento::{RegistrarSuprimento, SuprimentoFoiRegistrado};
pub use renegociar_titulo::{RenegociarTitulo, TituloFoiRenegociado};

use cardeal_kernel::{Data, Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{Contas, Contraparte, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::caixa::{Caixa, ContasCaixa, SessaoCaixa};
use crate::eventos::{ParcelaBaixada, TituloLancado};
use crate::receituario::{baixar_parcela, lancar_titulo, Autoria, ContasBaixa, ContasTitulo};
use crate::repositorio::{BaixaGravada, RepositorioFinanceiro};
use crate::titulo::{ConstrutorTitulo, EspecieTitulo};

/// Extrai a [`Autoria`] de um [`Ctx`] de comando.
fn autoria_de(ctx: &Ctx) -> Autoria {
    Autoria {
        usuario: ctx.usuario,
        dispositivo: ctx.dispositivo,
        agora: ctx.agora,
        fuso: ctx.fuso,
    }
}

fn especie_texto(e: EspecieTitulo) -> &'static str {
    match e {
        EspecieTitulo::Receber => "Receber",
        EspecieTitulo::Pagar => "Pagar",
    }
}

/// Os dados que os dois `LancarTitulo*` passam ao corpo comum — e que qualquer outro
/// módulo (`os`, `vendas`, `compras`, todos já dependentes de `mod-financeiro` no
/// manifesto) usa para lançar o próprio título direto, na mesma transação, chamando
/// [`lancar_titulo_comum`] em vez de ir pelo despacho (que exigiria uma segunda checagem
/// de permissão redundante e serialização sem necessidade — ver `docs/contratos-internos.md`
/// §7 regra 2: a proibição é sobre SQL entre módulos, não sobre função pública de crate).
pub struct DadosLancamentoTitulo {
    /// A receber ou a pagar.
    pub especie: EspecieTitulo,
    /// Com quem — cliente, fornecedor, funcionário ou sócio.
    pub contraparte: Contraparte,
    /// O valor total do título (soma das parcelas).
    pub valor_total: Dinheiro,
    /// Data de emissão.
    pub emissao: Data,
    /// Quantas parcelas (1..=360).
    pub parcelas: u16,
    /// O vencimento da primeira parcela.
    pub primeiro_vencimento: Data,
    /// Dias entre parcelas consecutivas.
    pub intervalo_dias: i32,
    /// Observação livre.
    pub observacao: Option<String>,
    /// Categoria de relatório, opcional (`docs/modulos/financeiro.md` §11.9) — não afeta a
    /// contabilização, só agregações por categoria/mês.
    pub categoria: Option<Id>,
    /// O módulo de origem (`"avulso"`, `"os"`, `"vendas"`, `"compras"`…).
    pub origem_modulo: &'static str,
    /// O agregado de origem (a OS, o pedido, a nota de entrada…), quando houver.
    pub origem_id: Option<Id>,
    /// Clientes a receber (Receber) ou Fornecedores (Pagar).
    pub papel_contraparte: PapelConta,
    /// A conta de resultado: receita a creditar (Receber) ou despesa a debitar (Pagar).
    pub papel_resultado: PapelConta,
}

/// O que o corpo comum devolve: `(título, parcelas, lançamentos)`.
pub struct TituloGravado {
    /// O título criado.
    pub titulo: Id,
    /// As parcelas, em ordem.
    pub parcelas: Vec<Id>,
    /// Os lançamentos `Confirmado` gerados, um por parcela.
    pub lancamentos: Vec<Id>,
}

/// Carregar/validar/lançar/persistir/publicar de um título — igual para as duas espécies e
/// para qualquer módulo de origem (ver [`DadosLancamentoTitulo`]).
///
/// # Errors
/// Erro de domínio (`ValorInvalido`, `NumeroDeParcelasInvalido`, papel de conta não
/// mapeado…) — desfaz o `SAVEPOINT` da tarefa do chamador junto com o resto do comando.
pub fn lancar_titulo_comum(
    dados: DadosLancamentoTitulo,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<TituloGravado> {
    // 1. Validar (domínio puro): rateio que fecha ao centavo.
    let mut construtor = ConstrutorTitulo::novo(
        ctx.empresa,
        dados.especie,
        dados.contraparte,
        dados.valor_total,
        dados.emissao,
    )
    .origem(dados.origem_modulo, dados.origem_id)
    .parcelas(
        dados.parcelas,
        dados.primeiro_vencimento,
        dados.intervalo_dias,
    );
    if let Some(obs) = dados.observacao {
        construtor = construtor.observacao(obs);
    }
    if let Some(cat) = dados.categoria {
        construtor = construtor.categoria(cat);
    }
    let mut tcp = construtor.construir().map_err(|e| Erro::de_dominio(&e))?;

    // 2. Resolver contas por papel.
    let (conta_contraparte, conta_resultado) = {
        let repo = RepositorioRazao::novo(uow);
        let contas = Contas::nova(&repo, ctx.empresa);
        (
            contas
                .papel(dados.papel_contraparte)
                .map_err(|e| Erro::de_dominio(&e))?,
            contas
                .papel(dados.papel_resultado)
                .map_err(|e| Erro::de_dominio(&e))?,
        )
    };

    // 3. Receituário + Razão: um lançamento por parcela, vinculado a ela.
    let lancs = lancar_titulo(
        &tcp,
        ContasTitulo {
            contraparte: conta_contraparte,
            resultado: conta_resultado,
        },
        autoria_de(ctx),
    )
    .map_err(|e| Erro::de_dominio(&e))?;

    let mut lancamentos = Vec::with_capacity(lancs.len());
    {
        let mut repo = RepositorioRazao::novo(uow);
        for (parcela, lanc) in tcp.parcelas.iter_mut().zip(lancs) {
            let id = Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?;
            parcela.lancamento = Some(id);
            lancamentos.push(id);
        }
    }

    // 4. Persistir.
    RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;

    // 5. Publicar.
    let parcelas: Vec<Id> = tcp.parcelas.iter().map(|p| p.id).collect();
    uow.publicar(TituloLancado {
        titulo: tcp.titulo.id,
        especie: especie_texto(dados.especie),
        valor: tcp.titulo.valor_original,
        parcelas: u16::try_from(parcelas.len()).unwrap_or(u16::MAX),
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(TituloGravado {
        titulo: tcp.titulo.id,
        parcelas,
        lancamentos,
    })
}

/// Os dados que os dois `Baixar*` passam ao corpo comum.
#[derive(Clone, Copy)]
struct DadosBaixa {
    parcela: Id,
    valor: Dinheiro,
    data: Data,
    conta_destino: Option<Id>,
    /// A espécie esperada — o comando recusa a parcela se o título for da outra.
    especie: EspecieTitulo,
    /// Papel da conta de destino padrão (Caixa para receber, Bancos para pagar).
    papel_destino_padrao: PapelConta,
    /// Papel dos encargos (Receita financeira para receber, Despesa financeira para pagar).
    papel_encargos: PapelConta,
    /// Papel do desconto (Descontos concedidos para receber, Outras receitas para pagar).
    papel_desconto: PapelConta,
}

/// O que o corpo comum da baixa devolve.
struct BaixaFeita {
    baixa: Id,
    lancamento: Id,
    quitada: bool,
    saldo_restante: Dinheiro,
}

/// Carregar/validar/lançar/persistir/publicar de uma baixa — igual para as duas espécies.
fn baixar_parcela_comum(
    dados: DadosBaixa,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<BaixaFeita> {
    // 1. Carregar.
    let mut parcela = RepositorioFinanceiro::novo(uow)
        .buscar_parcela(dados.parcela)?
        .ok_or_else(|| Erro::nao_encontrado("parcela"))?;
    let titulo = RepositorioFinanceiro::novo(uow)
        .buscar_titulo(parcela.titulo)?
        .ok_or_else(|| Erro::nao_encontrado("título"))?;

    if titulo.especie != dados.especie {
        return Err(Erro::novo(
            cardeal_kernel::CodigoErro::REGRA_VIOLADA,
            format!(
                "esta parcela pertence a um título {} — use a baixa da outra espécie",
                titulo.especie.rotulo()
            ),
        ));
    }

    // 2. Validar (domínio puro): juros/multa/desconto na data, rateio da baixa.
    let plano = parcela
        .planejar_baixa(dados.data, dados.valor)
        .map_err(|e| Erro::de_dominio(&e))?;

    // 3. Resolver contas (só as que o plano move) e montar o lançamento.
    let contas_baixa = {
        let repo = RepositorioRazao::novo(uow);
        let contas = Contas::nova(&repo, ctx.empresa);
        let destino = match dados.conta_destino {
            Some(id) => id,
            None => contas
                .papel(dados.papel_destino_padrao)
                .map_err(|e| Erro::de_dominio(&e))?,
        };
        let encargos = if (plano.juros + plano.multa).e_positivo() {
            contas
                .papel(dados.papel_encargos)
                .map_err(|e| Erro::de_dominio(&e))?
        } else {
            Id::NULO
        };
        let desconto = if plano.desconto.e_positivo() {
            contas
                .papel(dados.papel_desconto)
                .map_err(|e| Erro::de_dominio(&e))?
        } else {
            Id::NULO
        };
        ContasBaixa {
            conta_destino: destino,
            contraparte: contas
                .papel(if dados.especie == EspecieTitulo::Receber {
                    PapelConta::ClientesAReceber
                } else {
                    PapelConta::Fornecedores
                })
                .map_err(|e| Erro::de_dominio(&e))?,
            encargos,
            desconto,
        }
    };

    let lanc = baixar_parcela(
        dados.especie,
        titulo.contraparte,
        &parcela,
        &plano,
        contas_baixa,
        autoria_de(ctx),
    )
    .map_err(|e| Erro::de_dominio(&e))?;

    let lancamento = {
        let mut repo = RepositorioRazao::novo(uow);
        Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
    };

    // 4. Persistir.
    parcela.aplicar_baixa(&plano);
    let baixa = BaixaGravada {
        id: Id::novo(),
        empresa: ctx.empresa,
        parcela: parcela.id,
        data: plano.data,
        valor_recebido: plano.valor_recebido,
        principal: plano.principal,
        juros: plano.juros,
        multa: plano.multa,
        desconto: plano.desconto,
        lancamento,
        estornada_em: None,
    };
    {
        let mut repo = RepositorioFinanceiro::novo(uow);
        repo.atualizar_parcela(&parcela)?;
        repo.inserir_baixa(&baixa)?;
    }

    // 5. Publicar.
    uow.publicar(ParcelaBaixada {
        parcela: parcela.id,
        titulo: titulo.id,
        valor_recebido: plano.valor_recebido,
        quitada: plano.quita,
        lancamento,
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(BaixaFeita {
        baixa: baixa.id,
        lancamento,
        quitada: plano.quita,
        saldo_restante: parcela.saldo_principal(),
    })
}

/// Resolve as [`ContasCaixa`] para as operações de rotina de um caixa — abrir, suprimento,
/// sangria. Só `caixa` (a conta própria do caixa físico) e `contrapartida` (Bancos/cofre)
/// importam para essas três; `quebra` e `sobra` só entram no fechamento, que resolve as suas
/// por conta própria — aqui ficam [`Id::NULO`], nunca lidas fora dele (mesmo truque de
/// sentinela usado em [`baixar_parcela_comum`] para encargos/desconto que a baixa não
/// precisou).
fn contas_caixa_rotina(
    uow: &mut UnidadeDeTrabalho,
    ctx: &Ctx,
    caixa: &Caixa,
) -> Resultado<ContasCaixa> {
    let repo = RepositorioRazao::novo(uow);
    let contrapartida = Contas::nova(&repo, ctx.empresa)
        .papel(PapelConta::Bancos)
        .map_err(|e| Erro::de_dominio(&e))?;
    Ok(ContasCaixa {
        caixa: caixa.conta_razao,
        contrapartida,
        quebra: Id::NULO,
        sobra: Id::NULO,
    })
}

/// Carrega a sessão pelo id e o caixa dono dela.
fn carregar_sessao_e_caixa(
    uow: &mut UnidadeDeTrabalho,
    sessao: Id,
) -> Resultado<(SessaoCaixa, Caixa)> {
    let s = RepositorioFinanceiro::novo(uow)
        .buscar_sessao(sessao)?
        .ok_or_else(|| Erro::nao_encontrado("sessão de caixa"))?;
    let c = RepositorioFinanceiro::novo(uow)
        .buscar_caixa(s.caixa)?
        .ok_or_else(|| Erro::nao_encontrado("caixa"))?;
    Ok((s, c))
}
