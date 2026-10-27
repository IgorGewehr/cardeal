//! A NFC-e das vendas do PDV: configuração (CSC no cofre, série, próximo número), o documento
//! criado na hora da venda (rápido, sem rede — **a venda nunca espera a SEFAZ**, `docs/10` §3)
//! e a **tarefa** que transmite a fila fora de qualquer transação.

use cardeal_fiscal::{AmbienteFiscal, Nfce, Nfe};
use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::documento::{DocumentoResumo, EstadoDocumento, ResumoFiscal};

/// A configuração da NFC-e, para a tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguracaoNfce {
    /// Emitir NFC-e automaticamente a cada venda do PDV.
    pub emitir_nas_vendas: bool,
    /// A série.
    pub serie: u32,
    /// O próximo número da série, no ambiente atual.
    pub proximo_numero: u64,
    /// O identificador do CSC.
    pub csc_id: String,
    /// Se o token do CSC está guardado.
    pub csc_cadastrado: bool,
    /// O ambiente atual (a numeração é separada por ambiente).
    pub ambiente: AmbienteFiscal,
}

/// Configura a NFC-e. O token do CSC vai para o cofre; `None` mantém o guardado.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigurarNfce {
    /// Emitir a cada venda do PDV.
    pub emitir_nas_vendas: bool,
    /// A série (1 a 889).
    pub serie: u32,
    /// O próximo número no ambiente atual (≥ 1, e acima do maior já usado).
    pub proximo_numero: u64,
    /// O identificador do CSC (ex.: "000001").
    pub csc_id: String,
    /// O token do CSC; `None` mantém o guardado.
    pub csc_token: Option<String>,
}

impl std::fmt::Debug for ConfigurarNfce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigurarNfce")
            .field("emitir_nas_vendas", &self.emitir_nas_vendas)
            .field("serie", &self.serie)
            .field("proximo_numero", &self.proximo_numero)
            .field("csc_id", &self.csc_id)
            .field("csc_token", &self.csc_token.as_ref().map(|_| "[oculto]"))
            .finish()
    }
}

impl Drop for ConfigurarNfce {
    fn drop(&mut self) {
        if let Some(t) = self.csc_token.as_mut() {
            zeroize::Zeroize::zeroize(t);
        }
    }
}

impl Comando for ConfigurarNfce {
    type Saida = ConfiguracaoNfce;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<ConfiguracaoNfce> {
        sqlite::configurar(&self, ctx, uow)
    }
}

/// A configuração atual da NFC-e.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ConfiguracaoDaNfce;

impl Consulta for ConfiguracaoDaNfce {
    type Saida = ConfiguracaoNfce;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ConfiguracaoNfce> {
        sqlite::configuracao(ctx, c)
    }
}

/// Cria a NFC-e de uma venda finalizada do PDV: reserva o número e guarda a nota montada — ou,
/// se faltar algo (produto sem NCM, configuração incompleta), o documento já nasce rejeitado
/// com o que corrigir. Não fala com a rede. Repetir devolve o mesmo documento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfceDoCupom {
    /// O cupom (finalizado).
    pub cupom: Id,
}

impl Comando for EmitirNfceDoCupom {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::emitir_do_cupom(ctx, uow, self.cupom)
    }
}

/// Remonta um documento rejeitado (depois de corrigir o produto ou a configuração) e o põe de
/// volta na fila, com o mesmo número.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReenviarDocumento {
    /// O documento.
    pub documento: Id,
}

impl Comando for ReenviarDocumento {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::reenviar(ctx, uow, self.documento)
    }
}

/// Pede o cancelamento de uma NFC-e autorizada. A fila leva à SEFAZ (que aceita dentro do
/// prazo da UF — em geral 30 minutos para a NFC-e); recusado, a nota continua autorizada com o
/// motivo. A venda em si não é desfeita aqui.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarNfce {
    /// O documento (autorizado).
    pub documento: Id,
    /// A justificativa (15 a 255 caracteres).
    pub justificativa: String,
}

impl Comando for CancelarNfce {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::pedir(
            ctx,
            uow,
            self.documento,
            crate::documento::PedidoDocumento::Cancelar,
            &self.justificativa,
        )
    }
}

/// Pede a inutilização do número de uma NFC-e rejeitada que não vai mais ser enviada — sem
/// isso, o número fica como um buraco na série.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InutilizarNumero {
    /// O documento (rejeitado).
    pub documento: Id,
    /// A justificativa (15 a 255 caracteres).
    pub justificativa: String,
}

impl Comando for InutilizarNumero {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::pedir(
            ctx,
            uow,
            self.documento,
            crate::documento::PedidoDocumento::Inutilizar,
            &self.justificativa,
        )
    }
}

/// A venda do PDV foi estornada: a NFC-e dela tem de sair de cena. Marca o documento (de forma
/// permanente) e pede o que o estado dele pede — cancelar a autorizada, inutilizar a rejeitada
/// e a que nunca foi transmitida; a incerta e a pendente já tentada a fila resolve primeiro
/// (emite ou confere) e só então cancela ou inutiliza. Só vale para venda **estornada**: é a
/// consequência do estorno, por isso basta a permissão de emitir.
///
/// Devolve o documento, ou `None` se a venda não tinha NFC-e.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnularDocumentoDaVenda {
    /// O cupom estornado.
    pub cupom: Id,
    /// A justificativa que vai ao cancelamento ou à inutilização (15 a 255 caracteres).
    pub justificativa: String,
}

impl Comando for AnularDocumentoDaVenda {
    type Saida = Option<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Option<DocumentoResumo>> {
        sqlite::anular(ctx, uow, self.cupom, &self.justificativa)
    }
}

/// As NFC-e de várias vendas de uma vez (a lista "Vendas do dia" do PDV).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentosDasVendas {
    /// Os cupons.
    pub cupons: Vec<Id>,
}

impl Consulta for DocumentosDasVendas {
    type Saida = Vec<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<DocumentoResumo>> {
        crate::sql_documento::documentos_das_origens(
            c,
            ctx.empresa,
            sqlite::ORIGEM_PDV,
            &self.cupons,
        )
    }
}

/// Os documentos fiscais, os mais novos primeiro (até 500).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DocumentosFiscais {
    /// Só os deste estado.
    pub estado: Option<EstadoDocumento>,
}

impl Consulta for DocumentosFiscais {
    type Saida = Vec<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<DocumentoResumo>> {
        crate::sql_documento::documentos(c, ctx.empresa, self.estado, 500)
    }
}

/// O resumo da fila (a faixa de estado do PDV).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ResumoDaFilaFiscal;

impl Consulta for ResumoDaFilaFiscal {
    type Saida = ResumoFiscal;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ResumoFiscal> {
        let inicio = cardeal_kernel::Instante::de_data_hora(
            ctx.hoje(),
            cardeal_kernel::Hora::MEIA_NOITE,
            ctx.fuso,
        );
        let mut r = crate::sql_documento::resumo(c, ctx.empresa, inicio, ctx.agora)?;
        r.cartas_pendentes = crate::correcao::sqlite::quantas_pendentes(c, ctx.empresa)?;
        Ok(r)
    }
}

/// Põe na fila a busca, na consulta da nota, dos XMLs que faltam às notas de produção: o do
/// cancelamento que não veio na resposta (ou cuja resposta se perdeu) e o da nota confirmada
/// pela consulta cuja nota assinada ficou guardada. O pacote do contador pede antes de montar o
/// mês. Devolve quantas notas estão na busca; quem busca é a transmissão.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BuscarXmlsQueFaltam;

impl Comando for BuscarXmlsQueFaltam {
    type Saida = u32;
    const PERMISSAO: &'static str = "fiscal.emitir";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<u32> {
        crate::sql_documento::marcar_xmls_a_buscar(uow.conexao(), ctx.empresa)
    }
}

/// O documento com a nota e o XML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetalheDocumento {
    /// O resumo.
    pub resumo: DocumentoResumo,
    /// A NFC-e como foi montada (`None` se a montagem falhou, ou se é uma NF-e).
    pub nota: Option<Nfce>,
    /// A NF-e como foi montada (`None` se a montagem falhou, ou se é uma NFC-e).
    pub nfe: Option<Nfe>,
    /// O XML autorizado.
    pub xml: Option<String>,
    /// A nota assinada em contingência (ou na emissão incerta), enquanto o XML autorizado não
    /// vem — o DANFCE da contingência tira dela o QR Code.
    pub xml_assinado: Option<String>,
}

/// Um documento fiscal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentoFiscal {
    /// O documento.
    pub documento: Id,
}

impl Consulta for DocumentoFiscal {
    type Saida = DetalheDocumento;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<DetalheDocumento> {
        let d = crate::sql_documento::documento(c, ctx.empresa, self.documento)?
            .ok_or_else(|| cardeal_kernel::Erro::nao_encontrado("documento fiscal"))?;
        // A nota guardada é de um tipo ou de outro, conforme o modelo.
        let nfe = d.resumo.modelo == cardeal_fiscal::ModeloFiscal::Nfe;
        Ok(DetalheDocumento {
            nota: d
                .nota
                .as_deref()
                .filter(|_| !nfe)
                .and_then(|b| postcard::from_bytes(b).ok()),
            nfe: d
                .nota
                .as_deref()
                .filter(|_| nfe)
                .and_then(|b| postcard::from_bytes(b).ok()),
            resumo: d.resumo,
            xml: d.xml,
            xml_assinado: d.xml_assinado,
        })
    }
}

/// A NFC-e de um cupom, se houver.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentoDaVenda {
    /// O cupom.
    pub cupom: Id,
}

impl Consulta for DocumentoDaVenda {
    type Saida = Option<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<DocumentoResumo>> {
        crate::sql_documento::documento_da_origem(c, ctx.empresa, sqlite::ORIGEM_PDV, self.cupom)
    }
}

/// O nome da tarefa que transmite a fila.
pub const TAREFA_TRANSMITIR: &str = "fiscal.transmitir_pendentes";

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use std::collections::{HashMap, HashSet};
    use std::fmt::Write as _;
    use std::sync::Mutex;

    use cardeal_fiscal::{
        AmbienteFiscal, ChaveAcesso, Consumidor, Crt, Csc, DocumentoConsumidor, Emitente,
        EnderecoEmitente, EntradaEmContingencia, MeioPagamento, ModeloFiscal, Nfce, PortaFiscal,
    };
    use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado, Uf};
    use cardeal_modkit::{ContextoTarefa, Ctx};
    use cardeal_storage::{Cofre, Segredo, UnidadeDeTrabalho};
    use rusqlite::Connection;

    use super::{ConfiguracaoNfce, ConfigurarNfce};
    use crate::documento::{
        decidir, decidir_busca, decidir_evento, em_contingencia, esperar,
        justificativa_da_contingencia, nao_reemitir, pede_contingencia, pede_substituicao,
        pedido_da_substituida, Busca, Desfecho, DocumentoResumo, EstadoDocumento, Mudanca,
        PedidoDocumento, JUSTIFICATIVA_DA_SUBSTITUICAO,
    };
    use crate::montagem::{marcar_homologacao, montar_nfce, DadosDaVenda, ItemDaVenda};
    use crate::sql_documento as sqld;

    pub(crate) const ORIGEM_PDV: &str = "pdv";
    const MODELO: ModeloFiscal = ModeloFiscal::Nfce;

    fn contexto_csc(empresa: Id) -> Vec<u8> {
        let mut c = b"fiscal.csc/".to_vec();
        c.extend_from_slice(&empresa.em_bytes());
        c
    }

    pub(super) fn configurar(
        cmd: &ConfigurarNfce,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<ConfiguracaoNfce> {
        let c = uow.conexao();
        if !(1..=889).contains(&cmd.serie) {
            return Err(
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "A série da NFC-e vai de 1 a 889")
                    .no_campo("serie"),
            );
        }
        if cmd.proximo_numero == 0 || cmd.proximo_numero > 999_999_999 {
            return Err(Erro::novo(
                CodigoErro::FORA_DA_FAIXA,
                "O próximo número vai de 1 a 999.999.999",
            )
            .no_campo("proximo_numero"));
        }
        let csc_id = cmd.csc_id.trim();
        if !csc_id.is_empty() && (csc_id.len() > 6 || !csc_id.bytes().all(|b| b.is_ascii_digit())) {
            return Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "O identificador do CSC tem até 6 dígitos (ex.: 000001)",
            )
            .no_campo("csc_id"));
        }
        let atual = sqld::config_nfce(c, ctx.empresa)?;
        let token = cmd
            .csc_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let tera_token = token.is_some() || atual.csc_selado.is_some();
        if cmd.emitir_nas_vendas && (csc_id.is_empty() || !tera_token) {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "Para emitir NFC-e nas vendas, informe o CSC (identificador e token) da SEFAZ",
            )
            .no_campo("csc_id"));
        }
        crate::terminal::sqlite::exigir_serie_livre(c, ctx.empresa, cmd.serie)?;
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        if let Some(maior) = sqld::maior_numero(c, ctx.empresa, MODELO, cmd.serie, ambiente)? {
            if cmd.proximo_numero <= maior {
                return Err(Erro::novo(
                    CodigoErro::REGRA_VIOLADA,
                    format!(
                        "O número {maior} já foi usado na série {} — o próximo precisa ser maior",
                        cmd.serie
                    ),
                )
                .no_campo("proximo_numero"));
            }
        }
        let selado = match token {
            Some(t) => Some(
                ctx.recurso::<Cofre>()?
                    .selar(t.as_bytes(), &contexto_csc(ctx.empresa)),
            ),
            None => None,
        };
        sqld::gravar_config_nfce(
            c,
            ctx.empresa,
            cmd.emitir_nas_vendas,
            cmd.serie,
            csc_id,
            selado.as_deref(),
        )?;
        sqld::definir_proximo(
            c,
            ctx.empresa,
            MODELO,
            cmd.serie,
            ambiente,
            cmd.proximo_numero,
        )?;
        configuracao(ctx, c)
    }

    pub(super) fn configuracao(ctx: &Ctx, c: &Connection) -> Resultado<ConfiguracaoNfce> {
        let g = sqld::config_nfce(c, ctx.empresa)?;
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        Ok(ConfiguracaoNfce {
            emitir_nas_vendas: g.emitir,
            serie: g.serie,
            proximo_numero: sqld::proximo_numero(c, ctx.empresa, MODELO, g.serie, ambiente)?,
            csc_id: g.csc_id.unwrap_or_default(),
            csc_cadastrado: g.csc_selado.is_some(),
            ambiente,
        })
    }

    /// O emitente a partir dos dados da empresa — ou o que falta.
    pub(crate) fn emitente(
        c: &Connection,
        empresa: Id,
    ) -> Resultado<Result<Emitente, Vec<String>>> {
        let dados = mod_empresa::sql::dados(c, empresa)?;
        let Ok(fiscais) = mod_empresa::sql::dados_fiscais(c, empresa)?.normalizado() else {
            return Ok(Err(vec![
                "os dados fiscais da empresa estão incompletos (Configurações › Fiscal)".into(),
            ]));
        };
        let Some(uf) = fiscais.uf() else {
            return Ok(Err(vec!["a UF da empresa é inválida".into()]));
        };
        let Ok(cnpj) = cardeal_kernel::Cnpj::novo(&dados.cnpj) else {
            return Ok(Err(vec!["o CNPJ da empresa é inválido".into()]));
        };
        let crt = match dados.regime.as_str() {
            "LucroPresumido" | "LucroReal" => Crt::RegimeNormal,
            _ => Crt::SimplesNacional,
        };
        let telefone: String = mod_empresa::sql::identidade(c, empresa)?
            .telefone
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let e = fiscais.endereco;
        Ok(Ok(Emitente {
            cnpj,
            razao_social: dados.razao_social.trim().chars().take(60).collect(),
            nome_fantasia: dados.nome_fantasia.trim().chars().take(60).collect(),
            inscricao_estadual: fiscais.inscricao_estadual,
            inscricao_municipal: Some(fiscais.inscricao_municipal).filter(|s| !s.is_empty()),
            crt,
            endereco: EnderecoEmitente {
                logradouro: e.logradouro,
                numero: e.numero,
                complemento: e.complemento,
                bairro: e.bairro,
                codigo_municipio: e.codigo_municipio,
                municipio: e.municipio,
                uf,
                cep: e.cep,
                telefone: Some(telefone).filter(|t| (10..=14).contains(&t.len())),
            },
        }))
    }

    /// Monta a nota de um cupom com a série/número dados — ou o que impede.
    fn montar_do_cupom(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        cupom: Id,
        serie: u32,
        numero: u64,
        ambiente: AmbienteFiscal,
    ) -> Resultado<(cardeal_kernel::Dinheiro, Result<Nfce, Vec<String>>)> {
        let cupom = mod_pdv::RepositorioPdv::novo(uow)
            .buscar_cupom(cupom)?
            .ok_or_else(|| Erro::nao_encontrado("cupom"))?;
        if cupom.estado != mod_pdv::EstadoCupom::Finalizado {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Só uma venda finalizada vira NFC-e",
            ));
        }
        let c = uow.conexao();
        let itens: Vec<ItemDaVenda> = cupom
            .itens
            .iter()
            .filter(|i| !i.cancelado)
            .map(|i| ItemDaVenda {
                produto: i.produto,
                quantidade: i.quantidade,
                preco_unitario: i.preco_unitario,
                valor_bruto: i.total_bruto(),
                desconto: i.desconto_valor,
            })
            .collect();
        let pagamentos = cupom
            .pagamentos
            .iter()
            .map(|p| {
                let meio = match p.forma {
                    mod_pdv::FormaPagamentoPdv::Dinheiro => MeioPagamento::Dinheiro,
                    mod_pdv::FormaPagamentoPdv::Pix => MeioPagamento::Pix,
                    mod_pdv::FormaPagamentoPdv::Debito => MeioPagamento::CartaoDebito,
                    mod_pdv::FormaPagamentoPdv::Credito => MeioPagamento::CartaoCredito,
                    mod_pdv::FormaPagamentoPdv::Prazo => MeioPagamento::CreditoLoja,
                };
                (meio, p.valor)
            })
            .collect();
        let consumidor = match cupom.cliente {
            Some(cliente) => consumidor(c, cliente)?,
            None => None,
        };
        let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
        let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
        let mut problemas = pendencias_de_configuracao(ctx, c)?;
        let nota = match emitente(c, ctx.empresa)? {
            Ok(emitente) => montar_nfce(
                emitente,
                &DadosDaVenda {
                    itens,
                    pagamentos,
                    consumidor,
                },
                &produtos,
                serie,
                numero,
            ),
            Err(p) => Err(p),
        };
        let nota = match nota {
            Ok(mut n) if problemas.is_empty() => {
                if ambiente == AmbienteFiscal::Homologacao {
                    marcar_homologacao(&mut n);
                }
                Ok(n)
            }
            Ok(_) => Err(problemas),
            Err(mut p) => {
                problemas.append(&mut p);
                Err(problemas)
            }
        };
        Ok((cupom.total, nota))
    }

    fn consumidor(c: &Connection, cliente: Id) -> Resultado<Option<Consumidor>> {
        let documento = mod_clientes::documentos_da_pessoa(c, cliente)?
            .into_iter()
            .find_map(|d| match d.tipo {
                mod_clientes::TipoDocumento::Cpf => cardeal_kernel::Cpf::novo(&d.numero)
                    .ok()
                    .map(DocumentoConsumidor::Cpf),
                mod_clientes::TipoDocumento::Cnpj => cardeal_kernel::Cnpj::novo(&d.numero)
                    .ok()
                    .map(DocumentoConsumidor::Cnpj),
                _ => None,
            });
        let Some(documento) = documento else {
            return Ok(None);
        };
        let nome = mod_clientes::pessoa_por_id(c, cliente)?
            .map(|p| p.nome.trim().chars().take(60).collect::<String>())
            .filter(|n| n.len() >= 2);
        Ok(Some(Consumidor { documento, nome }))
    }

    /// O que falta na instalação para transmitir (certificado, CSC).
    fn pendencias_de_configuracao(ctx: &Ctx, c: &Connection) -> Resultado<Vec<String>> {
        let mut p = Vec::new();
        if crate::sql::certificado(c, ctx.empresa)?.is_none() {
            p.push("o certificado digital A1 não está instalado (Configurações › Fiscal)".into());
        }
        let config = sqld::config_nfce(c, ctx.empresa)?;
        if config.csc_selado.is_none() || config.csc_id.as_deref().is_none_or(str::is_empty) {
            p.push("o CSC da NFC-e não está cadastrado (Configurações › Fiscal)".into());
        }
        Ok(p)
    }

    pub(super) fn emitir_do_cupom(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        cupom: Id,
    ) -> Resultado<DocumentoResumo> {
        if let Some(d) = sqld::documento_da_origem(uow.conexao(), ctx.empresa, ORIGEM_PDV, cupom)? {
            return Ok(d);
        }
        // A série do terminal que vendeu, se ele tiver uma; senão a da empresa.
        let terminal = mod_pdv::RepositorioPdv::novo(uow)
            .buscar_cupom(cupom)?
            .map(|c| c.terminal);
        let (serie, ambiente) = {
            let c = uow.conexao();
            let do_terminal = match terminal {
                Some(t) => crate::terminal::sqlite::serie_do_terminal(c, ctx.empresa, t)?,
                None => None,
            };
            (
                do_terminal
                    .map_or_else(|| sqld::config_nfce(c, ctx.empresa).map(|c| c.serie), Ok)?,
                crate::sql::ambiente(c, ctx.empresa)?,
            )
        };
        let numero = sqld::reservar_numero(uow.conexao(), ctx.empresa, MODELO, serie, ambiente)?;
        let (total, nota) = montar_do_cupom(ctx, uow, cupom, serie, numero, ambiente)?;
        let (estado, blob, motivo) = match nota {
            Ok(n) => (
                EstadoDocumento::Pendente,
                Some(
                    postcard::to_stdvec(&n)
                        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
                ),
                None,
            ),
            Err(p) => (EstadoDocumento::Rejeitada, None, Some(p.join("; "))),
        };
        let id = Id::novo();
        let c = uow.conexao();
        sqld::inserir_documento(
            c,
            &sqld::NovoDocumento {
                id,
                empresa: ctx.empresa,
                modelo: MODELO,
                serie,
                numero,
                ambiente,
                origem_modulo: ORIGEM_PDV,
                origem: cupom,
                estado,
                total,
                nota: blob,
                motivo,
                agora: ctx.agora,
                substitui: None,
            },
        )?;
        sqld::documento_da_origem(c, ctx.empresa, ORIGEM_PDV, cupom)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }

    pub(super) fn reenviar(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        documento: Id,
    ) -> Resultado<DocumentoResumo> {
        let guardado = sqld::documento(uow.conexao(), ctx.empresa, documento)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))?;
        let (resumo, origem_modulo) = (guardado.resumo, guardado.origem_modulo);
        if resumo.estado != EstadoDocumento::Rejeitada {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!(
                    "Só nota rejeitada é reenviada (esta está {})",
                    resumo.estado.rotulo()
                ),
            ));
        }
        if resumo.anulada || resumo.pedido.is_some() {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Esta nota não se reenvia: a venda foi desfeita ou o número já vai ser inutilizado",
            ));
        }
        let (total, blob, motivo) = if origem_modulo == ORIGEM_PDV {
            let (total, nota) = montar_do_cupom(
                ctx,
                uow,
                resumo.origem,
                resumo.serie,
                resumo.numero,
                resumo.ambiente,
            )?;
            let (blob, motivo) = blob_ou_motivo(nota)?;
            (total, blob, motivo)
        } else if origem_modulo == crate::nfe_venda::ORIGEM_VENDAS {
            let (total, nota) = crate::nfe_venda::sqlite::montar_do_pedido(
                ctx,
                uow,
                resumo.origem,
                resumo.serie,
                resumo.numero,
            )?;
            let (blob, motivo) = blob_ou_motivo(nota)?;
            (total, blob, motivo)
        } else if origem_modulo == crate::devolucao_venda::ORIGEM_DEVOLUCAO_VENDA {
            let nota = crate::devolucao_venda::sqlite::montar(
                ctx,
                uow,
                resumo.origem,
                resumo.serie,
                resumo.numero,
            )?;
            let total = nota
                .as_ref()
                .map_or(resumo.total, cardeal_fiscal::Nfe::total);
            let (blob, motivo) = blob_ou_motivo(nota)?;
            (total, blob, motivo)
        } else if origem_modulo == crate::devolucao_compra::ORIGEM_DEVOLUCAO_COMPRA {
            // Os dados da devolução são do compras: a nota se remonta por lá.
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "A NF-e de devolução se reenvia pela devolução, em Compras (Emitir NF-e de \
                 devolução de novo, depois de corrigir)",
            ));
        } else {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Origem do documento desconhecida",
            ));
        };
        let c = uow.conexao();
        sqld::remontar(
            c,
            ctx.empresa,
            documento,
            blob.as_deref(),
            total,
            motivo.as_deref(),
            ctx.agora,
        )?;
        sqld::documento(c, ctx.empresa, documento)?
            .map(|d| d.resumo)
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }

    /// A nota em postcard — ou o que impede, numa frase.
    fn blob_ou_motivo<T: serde::Serialize>(
        nota: Result<T, Vec<String>>,
    ) -> Resultado<(Option<Vec<u8>>, Option<String>)> {
        Ok(match nota {
            Ok(n) => (
                Some(
                    postcard::to_stdvec(&n)
                        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
                ),
                None,
            ),
            Err(p) => (None, Some(p.join("; "))),
        })
    }

    /// A justificativa da SEFAZ: de 15 a 255 caracteres.
    fn justificativa_valida(justificativa: &str) -> Resultado<&str> {
        let justificativa = justificativa.trim();
        let n = justificativa.chars().count();
        if (15..=255).contains(&n) {
            Ok(justificativa)
        } else {
            Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "A justificativa precisa ter de 15 a 255 caracteres",
            )
            .no_campo("justificativa"))
        }
    }

    pub(super) fn anular(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        cupom: Id,
        justificativa: &str,
    ) -> Resultado<Option<DocumentoResumo>> {
        let justificativa = justificativa_valida(justificativa)?;
        let venda = mod_pdv::RepositorioPdv::novo(uow)
            .buscar_cupom(cupom)?
            .ok_or_else(|| Erro::nao_encontrado("venda"))?;
        if venda.estado != mod_pdv::EstadoCupom::Estornado {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "A venda não foi estornada: a NFC-e só se anula junto com o estorno da venda",
            ));
        }
        let c = uow.conexao();
        let Some(doc) = sqld::documento_da_origem(c, ctx.empresa, ORIGEM_PDV, cupom)? else {
            return Ok(None);
        };
        if !doc.anulada {
            let pedido = crate::documento::pedido_da_anulacao(doc.estado, doc.tentativas)
                .filter(|_| doc.pedido.is_none());
            sqld::anular(c, ctx.empresa, doc.id, justificativa, pedido, ctx.agora)?;
        }
        sqld::documento_da_origem(c, ctx.empresa, ORIGEM_PDV, cupom)
    }

    pub(super) fn pedir(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        documento: Id,
        pedido: PedidoDocumento,
        justificativa: &str,
    ) -> Resultado<DocumentoResumo> {
        let justificativa = justificativa_valida(justificativa)?;
        let c = uow.conexao();
        let guardado = sqld::documento(c, ctx.empresa, documento)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))?;
        let resumo = guardado.resumo;
        let exigido = match pedido {
            PedidoDocumento::Cancelar | PedidoDocumento::CancelarPorSubstituicao => {
                EstadoDocumento::Autorizada
            }
            PedidoDocumento::Inutilizar => EstadoDocumento::Rejeitada,
        };
        if resumo.estado != exigido {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                match pedido {
                    PedidoDocumento::Cancelar | PedidoDocumento::CancelarPorSubstituicao => {
                        "Só NFC-e autorizada é cancelada"
                    }
                    PedidoDocumento::Inutilizar => {
                        "Só o número de uma NFC-e rejeitada (que nunca foi autorizada) é inutilizado"
                    }
                },
            ));
        }
        if let Some(p) = resumo.pedido {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!("Já há um pedido para esta nota: {}", p.rotulo()),
            ));
        }
        sqld::pedir(
            c,
            ctx.empresa,
            documento,
            pedido,
            justificativa,
            exigido,
            ctx.agora,
        )?;
        sqld::documento(c, ctx.empresa, documento)?
            .map(|d| d.resumo)
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }

    /// As empresas com transmissão em andamento neste processo — duas tarefas ao mesmo tempo
    /// emitiriam a mesma nota duas vezes.
    static EM_ANDAMENTO: Mutex<Option<HashSet<Id>>> = Mutex::new(None);

    struct Guarda(Id);

    impl Guarda {
        fn tomar(empresa: Id) -> Option<Self> {
            let mut g = EM_ANDAMENTO
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            g.get_or_insert_with(HashSet::new)
                .insert(empresa)
                .then_some(Self(empresa))
        }
    }

    impl Drop for Guarda {
        fn drop(&mut self) {
            let mut g = EM_ANDAMENTO
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(s) = g.as_mut() {
                s.remove(&self.0);
            }
        }
    }

    fn csc(ctx: &Ctx, c: &Connection) -> Resultado<Option<Csc>> {
        let config = sqld::config_nfce(c, ctx.empresa)?;
        let (Some(id), Some(selado)) = (config.csc_id.filter(|i| !i.is_empty()), config.csc_selado)
        else {
            return Ok(None);
        };
        let aberto = ctx
            .recurso::<Cofre>()?
            .abrir(&selado, &contexto_csc(ctx.empresa))
            .map_err(|e| Erro::de_dominio(&e))?;
        let token = String::from_utf8(aberto.expor().clone())
            .map_err(|_| Erro::novo(CodigoErro::FALHA_INTERNA, "CSC guardado ilegível"))?;
        Ok(Some(Csc {
            id,
            token: Segredo::novo(token),
        }))
    }

    /// Uma porta por ambiente presente na fila.
    type Portas = HashMap<AmbienteFiscal, Box<dyn PortaFiscal>>;

    /// Transmite a fila: emite os pendentes e consulta os incertos, um a um, fora de qualquer
    /// transação; grava cada resultado na hora. Para quando a API não responde (os outros
    /// esperam a próxima rodada). Devolve um resumo.
    ///
    /// # Errors
    /// A instalação não consegue falar com a API (sem certificado, sem provedor, cofre de
    /// outra instalação) — nesse caso nada muda na fila.
    pub(crate) fn transmitir_pendentes(t: &ContextoTarefa<'_>) -> Resultado<String> {
        let empresa = t.ctx.empresa;
        let Some(_guarda) = Guarda::tomar(empresa) else {
            return Ok("já há uma transmissão em andamento".into());
        };
        let agora = Instante::agora();
        let fila = t.ler(|c| sqld::fila(c, empresa, agora, 20))?;
        let cartas = t.ler(|c| crate::correcao::sqlite::pendentes(c, empresa, agora, 10))?;
        if fila.is_empty() && cartas.is_empty() {
            return Ok("nada na fila".into());
        }
        let ambientes: HashSet<AmbienteFiscal> = fila
            .iter()
            .map(|d| d.ambiente)
            .chain(cartas.iter().map(|k| k.ambiente))
            .collect();
        let (portas, csc, uf): (Portas, Option<Csc>, Option<Uf>) = t.ler(|c| {
            let mut portas = HashMap::new();
            for a in ambientes {
                portas.insert(
                    a,
                    crate::provedor::porta_da_empresa_no_ambiente(&t.ctx, c, a)?,
                );
            }
            let csc = csc(&t.ctx, c)?;
            let uf = mod_empresa::sql::dados_fiscais(c, empresa)?.uf();
            Ok((portas, csc, uf))
        })?;
        let cnpj = t.ler(|c| {
            Ok(cardeal_kernel::Cnpj::novo(&mod_empresa::sql::dados(c, empresa)?.cnpj).ok())
        })?;
        let mut rodada = Rodada::default();
        for doc in fila {
            let Some(porta) = portas.get(&doc.ambiente) else {
                continue;
            };
            let ctx = Contexto {
                porta: porta.as_ref(),
                csc: csc.as_ref(),
                uf,
                cnpj,
            };
            if processar(t, &ctx, doc, &mut rodada)? {
                return Ok(resumo_da_rodada(&rodada));
            }
        }
        for carta in cartas {
            let Some(porta) = portas.get(&carta.ambiente) else {
                continue;
            };
            if levar_carta(t, porta.as_ref(), &carta, uf, &mut rodada)? {
                break;
            }
        }
        Ok(resumo_da_rodada(&rodada))
    }

    /// Leva uma carta de correção à SEFAZ e grava o que veio. Devolve se a rodada para.
    fn levar_carta(
        t: &ContextoTarefa<'_>,
        porta: &dyn PortaFiscal,
        carta: &crate::correcao::sqlite::CartaNaFila,
        uf: Option<Uf>,
        rodada: &mut Rodada,
    ) -> Resultado<bool> {
        use crate::correcao::{decidir_carta, DestinoDaCarta};
        let desfecho = match (carta.chave.as_deref(), uf) {
            (Some(chave), Some(uf)) => match ChaveAcesso::nova(chave) {
                Ok(chave) => {
                    desfecho(porta.carta_de_correcao(&chave, &carta.texto, carta.sequencia, uf))
                }
                Err(e) => Desfecho::Recusado(e.to_string()),
            },
            _ => Desfecho::Recusado("a nota não tem chave, ou a empresa não tem a UF".into()),
        };
        let parar = matches!(desfecho, Desfecho::Indisponivel(_));
        let destino = decidir_carta(desfecho);
        match destino {
            DestinoDaCarta::Registrada { .. } => rodada.cartas.0 += 1,
            DestinoDaCarta::Recusada(_) => rodada.cartas.1 += 1,
            DestinoDaCarta::Esperar(_) => {}
        }
        let (empresa, id, tentativas) = (t.ctx.empresa, carta.id, carta.tentativas);
        t.escrever(move |uow| {
            crate::correcao::sqlite::aplicar(
                uow.conexao(),
                empresa,
                id,
                &destino,
                tentativas,
                Instante::agora(),
            )
        })?;
        Ok(parar)
    }

    /// O que a rodada fez, para o resumo.
    #[derive(Default)]
    struct Rodada {
        /// Cancelados, inutilizados, recusados.
        eventos: (u32, u32, u32),
        contagem: HashMap<EstadoDocumento, u32>,
        recuperados: u32,
        substituidas: u32,
        /// Cartas de correção registradas e recusadas.
        cartas: (u32, u32),
    }

    /// O que vale para a rodada inteira.
    struct Contexto<'a> {
        porta: &'a dyn PortaFiscal,
        csc: Option<&'a Csc>,
        uf: Option<Uf>,
        cnpj: Option<cardeal_kernel::Cnpj>,
    }

    /// Leva um documento à SEFAZ e grava o que veio. Devolve se a rodada para (a API não
    /// respondeu).
    fn processar(
        t: &ContextoTarefa<'_>,
        x: &Contexto<'_>,
        doc: sqld::NaFila,
        rodada: &mut Rodada,
    ) -> Resultado<bool> {
        let empresa = t.ctx.empresa;
        // Documento fechado com XML a buscar: só a consulta, sem mudar o estado.
        let fechado = !matches!(
            doc.estado,
            EstadoDocumento::Pendente | EstadoDocumento::Incerta | EstadoDocumento::Contingencia
        );
        if doc.pedido.is_none() && doc.buscar_xml && fechado {
            return Ok(match buscar_xmls(t, x.porta, &doc, x.uf)? {
                Some(n) => {
                    rodada.recuperados += n;
                    false
                }
                None => true,
            });
        }
        // O cancelamento por substituição espera a substituta autorizada (a SEFAZ exige).
        let substituta = if doc.pedido == Some(PedidoDocumento::CancelarPorSubstituicao) {
            let Some(chave) = substituta_autorizada(t, &doc)? else {
                return Ok(false);
            };
            Some(chave)
        } else {
            None
        };
        let (mudanca, parar, api_respondeu) =
            decidir_documento(t, x, &doc, substituta.as_deref(), rodada);
        if doc.pedido.is_none() {
            *rodada.contagem.entry(mudanca.estado).or_default() += 1;
        }
        // A incerta que segue sem resposta, com o cliente ainda no balcão: a substituta.
        let idade = doc.criado_em.micros_ate(Instante::agora()) / 1_000_000;
        let substituir = doc.pedido.is_none()
            && doc.modelo == ModeloFiscal::Nfce
            && doc.nota.is_some()
            && pede_substituicao(
                doc.estado,
                &mudanca,
                api_respondeu,
                idade,
                doc.substituida_por.is_some(),
            );
        let pedido_novo = pedido_que_nasce(&doc, &mudanca);
        let justificativa = if doc.substituida_por.is_some() {
            JUSTIFICATIVA_DA_SUBSTITUICAO.to_owned()
        } else {
            doc.justificativa.clone().unwrap_or_default()
        };
        let (id, estado_lido) = (doc.id, doc.estado);
        let criou_substituta = t.escrever(move |uow| {
            let agora = Instante::agora();
            let aplicou =
                sqld::aplicar_mudanca(uow.conexao(), empresa, id, estado_lido, &mudanca, agora)?;
            if let (true, Some(p)) = (aplicou, pedido_novo) {
                sqld::pedir(
                    uow.conexao(),
                    empresa,
                    id,
                    p,
                    &justificativa,
                    mudanca.estado,
                    agora,
                )?;
            }
            if aplicou && substituir {
                return criar_substituta(uow, empresa, &doc, agora);
            }
            Ok(false)
        })?;
        if criou_substituta {
            rodada.substituidas += 1;
        }
        Ok(parar)
    }

    /// A chave da substituta, quando ela já está autorizada; senão grava a espera e devolve
    /// `None`.
    fn substituta_autorizada(
        t: &ContextoTarefa<'_>,
        doc: &sqld::NaFila,
    ) -> Resultado<Option<String>> {
        let empresa = t.ctx.empresa;
        let sub = match doc.substituida_por {
            Some(s) => t.ler(|c| sqld::estado_e_chave(c, empresa, s))?,
            None => None,
        };
        if let Some((EstadoDocumento::Autorizada, Some(chave))) = sub {
            return Ok(Some(chave));
        }
        let m = esperar(
            doc.estado,
            doc.tentativas,
            "Esperando a NFC-e substituta ser autorizada para cancelar esta",
        );
        let (id, lido) = (doc.id, doc.estado);
        t.escrever(move |uow| {
            sqld::aplicar_mudanca(uow.conexao(), empresa, id, lido, &m, Instante::agora())
        })?;
        Ok(None)
    }

    /// O pedido que nasce do desfecho: a substituída resolvida (cancelar por substituição ou
    /// inutilizar) ou a venda desfeita (cancelar ou inutilizar).
    fn pedido_que_nasce(doc: &sqld::NaFila, mudanca: &Mudanca) -> Option<PedidoDocumento> {
        if doc.pedido.is_some() || mudanca.estado == doc.estado {
            return None;
        }
        if doc.substituida_por.is_some() {
            pedido_da_substituida(mudanca.estado)
        } else if doc.anulada {
            crate::documento::pedido_da_anulacao(mudanca.estado, 0)
                .filter(|_| mudanca.estado != EstadoDocumento::Pendente)
        } else {
            None
        }
    }

    /// Leva o documento à SEFAZ e decide: o pedido, a substituta (sempre em contingência) ou a
    /// emissão/consulta. Devolve a mudança, se a rodada para e se a API respondeu.
    fn decidir_documento(
        t: &ContextoTarefa<'_>,
        x: &Contexto<'_>,
        doc: &sqld::NaFila,
        substituta: Option<&str>,
        rodada: &mut Rodada,
    ) -> (Mudanca, bool, bool) {
        if let Some(pedido) = doc.pedido {
            let desfecho =
                desfecho_do_pedido(x.porta, pedido, doc, x.uf, x.cnpj, t.ctx.fuso, substituta);
            let parar = matches!(desfecho, Desfecho::Indisponivel(_));
            (
                mudanca_do_pedido(pedido, doc, desfecho, &mut rodada.eventos),
                parar,
                !parar,
            )
        } else if doc.estado == EstadoDocumento::Pendente && doc.substitui.is_some() {
            // A substituta sai sempre em contingência: a SEFAZ exige `tpEmis` 9 no 110112.
            (contingencia_da_substituta(x, doc, t.ctx.fuso), false, true)
        } else {
            let desfecho = desfecho_da_emissao(x.porta, doc, x.csc, x.uf);
            let parar = matches!(desfecho, Desfecho::Indisponivel(_));
            let api_respondeu = matches!(desfecho, Desfecho::Resposta(_));
            let mut m = mudanca_da_emissao(x.porta, doc, x.csc, desfecho, t.ctx.fuso);
            if doc.substituida_por.is_some() {
                m = nao_reemitir(m);
            }
            (m, parar, api_respondeu)
        }
    }

    /// Cria a substituta de `doc` (a mesma nota, outro número da mesma série) e a marca como
    /// substituída. Devolve se criou.
    fn criar_substituta(
        uow: &mut UnidadeDeTrabalho,
        empresa: Id,
        doc: &sqld::NaFila,
        agora: Instante,
    ) -> Resultado<bool> {
        let Some(blob) = doc.nota.as_deref() else {
            return Ok(false);
        };
        let Ok(mut nota) = postcard::from_bytes::<Nfce>(blob) else {
            return Ok(false);
        };
        let c = uow.conexao();
        let numero = sqld::reservar_numero(c, empresa, MODELO, doc.serie, doc.ambiente)?;
        nota.numero = numero;
        let blob = postcard::to_stdvec(&nota)
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
        let id_da_substituta = Id::novo();
        // Primeiro a original deixa de ser a vigente da venda (a origem é única entre elas).
        if !sqld::marcar_substituida(c, empresa, doc.id, id_da_substituta)? {
            return Ok(false);
        }
        sqld::inserir_documento(
            c,
            &sqld::NovoDocumento {
                id: id_da_substituta,
                empresa,
                modelo: MODELO,
                serie: doc.serie,
                numero,
                ambiente: doc.ambiente,
                origem_modulo: &doc.origem_modulo,
                origem: doc.origem,
                estado: EstadoDocumento::Pendente,
                total: doc.total,
                nota: Some(blob),
                motivo: Some(format!(
                    "Substitui a NFC-e {}-{}, que ficou sem resposta da SEFAZ",
                    doc.serie, doc.numero
                )),
                agora,
                substitui: Some(doc.id),
            },
        )?;
        Ok(true)
    }

    /// A substituta vai direto para a contingência; sem a assinatura, espera como pendente.
    fn contingencia_da_substituta(
        x: &Contexto<'_>,
        doc: &sqld::NaFila,
        fuso: cardeal_kernel::Fuso,
    ) -> Mudanca {
        let r = cardeal_fiscal::RetornoSefaz {
            situacao: cardeal_fiscal::SituacaoRetorno::Erro,
            codigo: "SUB".into(),
            motivo: "a NFC-e anterior desta venda ficou sem resposta".into(),
            chave: None,
            protocolo: None,
            recebido_em: None,
            xml: None,
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            // "Não chegou": sem a assinatura, a pendente espera em vez de virar rejeitada.
            nao_enviado: true,
        };
        contingencia(x.porta, doc, x.csc, Desfecho::Resposta(r), fuso)
    }

    /// O que gravar depois de levar um pedido (cancelar, inutilizar) — e a conta da rodada:
    /// cancelados, inutilizados, recusados.
    fn mudanca_do_pedido(
        pedido: PedidoDocumento,
        doc: &sqld::NaFila,
        desfecho: Desfecho,
        eventos: &mut (u32, u32, u32),
    ) -> Mudanca {
        let m = decidir_evento(pedido, doc.estado, doc.tentativas, desfecho);
        match m.estado {
            EstadoDocumento::Cancelada => eventos.0 += 1,
            EstadoDocumento::Inutilizada => eventos.1 += 1,
            _ if m.limpar_pedido => eventos.2 += 1,
            _ => {}
        }
        m
    }

    /// O que gravar depois de emitir, transmitir ou consultar: a decisão — ou, quando a SEFAZ
    /// está fora e a nota não chegou a ela, a contingência, já.
    fn mudanca_da_emissao(
        porta: &dyn PortaFiscal,
        doc: &sqld::NaFila,
        csc: Option<&Csc>,
        desfecho: Desfecho,
        fuso: cardeal_kernel::Fuso,
    ) -> Mudanca {
        // A contingência off-line é da NFC-e (a da NF-e é outra, a SVC).
        if doc.modelo == ModeloFiscal::Nfce && pede_contingencia(doc.estado, &desfecho) {
            contingencia(porta, doc, csc, desfecho, fuso)
        } else {
            decidir(doc.estado, doc.tentativas, desfecho)
        }
    }

    /// Assina em contingência a nota pendente que a SEFAZ fora não recebeu. Se a assinatura
    /// falha, a nota espera como pendente — com o motivo dos dois.
    fn contingencia(
        porta: &dyn PortaFiscal,
        doc: &sqld::NaFila,
        csc: Option<&Csc>,
        desfecho: Desfecho,
        fuso: cardeal_kernel::Fuso,
    ) -> Mudanca {
        let assinada = match (&desfecho, &doc.nota, csc) {
            (Desfecho::Resposta(r), Some(blob), Some(csc)) => {
                let inicio = Instante::agora();
                postcard::from_bytes::<Nfce>(blob)
                    .map_err(|e| e.to_string())
                    .and_then(|nota| {
                        porta
                            .preparar_nfce_em_contingencia(
                                &nota,
                                csc,
                                &EntradaEmContingencia {
                                    inicio,
                                    justificativa: justificativa_da_contingencia(r),
                                    fuso,
                                },
                            )
                            .map_err(|e| e.mensagem)
                    })
                    .map(|n| em_contingencia(n, r, inicio))
            }
            _ => Err("sem a nota guardada ou o CSC".into()),
        };
        match assinada {
            Ok(m) => m,
            Err(falha) => {
                let mut m = decidir(doc.estado, doc.tentativas, desfecho);
                m.motivo = Some(format!(
                    "{} — e a contingência não saiu: {falha}",
                    m.motivo.unwrap_or_default()
                ));
                m
            }
        }
    }

    /// A frase do fim da rodada: as emissões, os pedidos, as substituições e os XMLs recuperados.
    fn resumo_da_rodada(rodada: &Rodada) -> String {
        let (cancelados, inutilizados, recusados) = rodada.eventos;
        let recuperados = rodada.recuperados;
        let n = |e| rodada.contagem.get(&e).copied().unwrap_or(0);
        let mut resumo = format!(
            "{} autorizada(s), {} rejeitada(s), {} na fila",
            n(EstadoDocumento::Autorizada),
            n(EstadoDocumento::Rejeitada) + n(EstadoDocumento::Denegada),
            n(EstadoDocumento::Pendente) + n(EstadoDocumento::Incerta)
        );
        if n(EstadoDocumento::Contingencia) > 0 {
            let _ = write!(
                resumo,
                "; {} em contingência",
                n(EstadoDocumento::Contingencia)
            );
        }
        if cancelados + inutilizados + recusados > 0 {
            let _ = write!(
                resumo,
                "; {cancelados} cancelada(s), {inutilizados} inutilizada(s), {recusados} pedido(s) recusado(s)"
            );
        }
        if rodada.cartas.0 + rodada.cartas.1 > 0 {
            let _ = write!(
                resumo,
                "; {} carta(s) de correção registrada(s), {} recusada(s)",
                rodada.cartas.0, rodada.cartas.1
            );
        }
        if rodada.substituidas > 0 {
            let _ = write!(
                resumo,
                "; {} sem resposta substituída(s) em contingência",
                rodada.substituidas
            );
        }
        if recuperados > 0 {
            let _ = write!(resumo, "; {recuperados} XML(s) recuperado(s) na SEFAZ");
        }
        resumo
    }

    /// Busca os XMLs que faltam a um documento fechado e grava o que veio: devolve quantos
    /// vieram, ou `None` se a API não respondeu (a rodada para).
    fn buscar_xmls(
        t: &ContextoTarefa<'_>,
        porta: &dyn PortaFiscal,
        doc: &sqld::NaFila,
        uf: Option<Uf>,
    ) -> Resultado<Option<u32>> {
        let busca = decidir_busca(desfecho_da_busca(porta, doc, uf));
        let vieram = match &busca {
            Busca::Encerrar {
                xml, xml_evento, ..
            } => Some(u32::from(xml.is_some()) + u32::from(xml_evento.is_some())),
            Busca::TentarDeNovo { gasta: true, .. } => Some(0),
            Busca::TentarDeNovo { gasta: false, .. } => None,
        };
        let (empresa, id) = (t.ctx.empresa, doc.id);
        t.escrever(move |uow| {
            sqld::aplicar_busca(uow.conexao(), empresa, id, &busca, Instante::agora())
        })?;
        Ok(vieram)
    }

    /// Consulta um documento fechado para buscar os XMLs que faltam (com a nota assinada, se
    /// ficou guardada).
    fn desfecho_da_busca(porta: &dyn PortaFiscal, doc: &sqld::NaFila, uf: Option<Uf>) -> Desfecho {
        let (Some(chave), Some(uf)) = (doc.chave.as_deref(), uf) else {
            return Desfecho::Recusado("sem a chave da nota ou a UF da empresa".into());
        };
        match ChaveAcesso::nova(chave) {
            Ok(chave) => {
                desfecho(porta.consultar_documento(&chave, uf, doc.xml_assinado.as_deref()))
            }
            Err(e) => Desfecho::Recusado(e.to_string()),
        }
    }

    /// Emite (pendente) ou consulta (incerta) um documento.
    fn desfecho_da_emissao(
        porta: &dyn PortaFiscal,
        doc: &sqld::NaFila,
        csc: Option<&Csc>,
        uf: Option<Uf>,
    ) -> Desfecho {
        match (doc.estado, &doc.nota, &doc.chave, csc, uf) {
            // A NF-e não tem CSC.
            (EstadoDocumento::Pendente, Some(blob), _, _, _) if doc.modelo == ModeloFiscal::Nfe => {
                match postcard::from_bytes::<cardeal_fiscal::Nfe>(blob) {
                    Ok(nota) => desfecho(porta.emitir_nfe(&nota)),
                    Err(e) => Desfecho::Recusado(format!("a nota guardada está ilegível: {e}")),
                }
            }
            (EstadoDocumento::Pendente, Some(blob), _, Some(csc), _) => {
                match postcard::from_bytes::<Nfce>(blob) {
                    Ok(nota) => desfecho(porta.emitir_nfce(&nota, csc)),
                    Err(e) => Desfecho::Recusado(format!("a nota guardada está ilegível: {e}")),
                }
            }
            (EstadoDocumento::Pendente, _, _, None, _) => Desfecho::Recusado(
                "o CSC da NFC-e não está cadastrado (Configurações › Fiscal)".into(),
            ),
            // A contingência: o mesmo XML assinado, quantas vezes for preciso.
            (EstadoDocumento::Contingencia, _, _, _, Some(uf)) => match doc.xml_assinado.as_deref()
            {
                Some(xml) => desfecho(porta.transmitir_nfce_em_contingencia(xml, uf)),
                None => {
                    Desfecho::Recusado("a nota assinada em contingência não está guardada".into())
                }
            },
            (EstadoDocumento::Incerta, _, Some(chave), _, Some(uf)) => {
                match ChaveAcesso::nova(chave) {
                    Ok(chave) => {
                        desfecho(porta.consultar_documento(&chave, uf, doc.xml_assinado.as_deref()))
                    }
                    Err(e) => Desfecho::Recusado(e.to_string()),
                }
            }
            _ => Desfecho::Recusado("documento sem nota ou chave para transmitir".into()),
        }
    }

    /// Leva um pedido (cancelar/inutilizar) à porta.
    fn desfecho_do_pedido(
        porta: &dyn PortaFiscal,
        pedido: PedidoDocumento,
        doc: &sqld::NaFila,
        uf: Option<Uf>,
        cnpj: Option<cardeal_kernel::Cnpj>,
        fuso: cardeal_kernel::Fuso,
        substituta: Option<&str>,
    ) -> Desfecho {
        let (Some(justificativa), Some(uf)) = (doc.justificativa.as_deref(), uf) else {
            return Desfecho::Recusado("faltam a justificativa ou a UF da empresa".into());
        };
        match pedido {
            PedidoDocumento::Cancelar => {
                let (Some(chave), Some(protocolo)) =
                    (doc.chave.as_deref(), doc.protocolo.as_deref())
                else {
                    return Desfecho::Recusado(
                        "a nota não tem chave e protocolo para cancelar".into(),
                    );
                };
                match ChaveAcesso::nova(chave) {
                    Ok(chave) => desfecho(porta.cancelar(&chave, protocolo, justificativa, uf)),
                    Err(e) => Desfecho::Recusado(e.to_string()),
                }
            }
            PedidoDocumento::CancelarPorSubstituicao => {
                let (Some(chave), Some(protocolo), Some(sub)) =
                    (doc.chave.as_deref(), doc.protocolo.as_deref(), substituta)
                else {
                    return Desfecho::Recusado(
                        "a nota não tem chave e protocolo, ou a substituta não tem chave".into(),
                    );
                };
                match (ChaveAcesso::nova(chave), ChaveAcesso::nova(sub)) {
                    (Ok(chave), Ok(sub)) => desfecho(porta.cancelar_por_substituicao(
                        &chave,
                        protocolo,
                        justificativa,
                        uf,
                        &sub,
                    )),
                    (Err(e), _) | (_, Err(e)) => Desfecho::Recusado(e.to_string()),
                }
            }
            PedidoDocumento::Inutilizar => {
                let Some(cnpj) = cnpj else {
                    return Desfecho::Recusado("o CNPJ da empresa é inválido".into());
                };
                let ano =
                    u16::try_from(doc.criado_em.data(fuso).ano().rem_euclid(100)).unwrap_or(0);
                desfecho(porta.inutilizar(&cardeal_fiscal::PedidoInutilizacao {
                    cnpj,
                    modelo: doc.modelo,
                    serie: doc.serie,
                    inicio: doc.numero,
                    fim: doc.numero,
                    ano,
                    justificativa: justificativa.to_string(),
                    uf,
                }))
            }
        }
    }

    fn desfecho(r: Resultado<cardeal_fiscal::RetornoSefaz>) -> Desfecho {
        match r {
            Ok(r) => Desfecho::Resposta(r),
            Err(e) if e.codigo == CodigoErro::FISCAL_INDISPONIVEL => {
                Desfecho::Indisponivel(e.mensagem)
            }
            Err(e) => Desfecho::Recusado(e.mensagem),
        }
    }
}
