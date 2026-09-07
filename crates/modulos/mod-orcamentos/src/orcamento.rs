//! O orçamento comercial e seu ciclo de vida.
//!
//! Domínio puro: as transições só validam e mutam `self`; quem numera, persiste e abre a OS
//! é o comando.

use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Instante, Percentual, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroOrcamentos;
use crate::item::ItemOrcamento;

/// O estado de um [`Orcamento`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoOrcamento {
    /// Em edição — ainda não foi para o cliente.
    Rascunho,
    /// Entregue ao cliente, aguardando decisão.
    Enviado,
    /// O cliente aprovou.
    Aprovado,
    /// O cliente recusou. Terminal.
    Recusado,
    /// A validade venceu sem decisão. Terminal (pode ser duplicado).
    Expirado,
    /// Cancelado internamente. Terminal.
    Cancelado,
    /// Virou uma ordem de serviço. Terminal.
    Convertido,
}

impl EstadoOrcamento {
    /// O rótulo persistido (igual ao `CHECK` da tabela).
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Rascunho => "Rascunho",
            Self::Enviado => "Enviado",
            Self::Aprovado => "Aprovado",
            Self::Recusado => "Recusado",
            Self::Expirado => "Expirado",
            Self::Cancelado => "Cancelado",
            Self::Convertido => "Convertido",
        }
    }

    /// O rótulo amigável em português.
    #[must_use]
    pub const fn rotulo_humano(self) -> &'static str {
        match self {
            Self::Rascunho => "Rascunho",
            Self::Enviado => "Enviado",
            Self::Aprovado => "Aprovado",
            Self::Recusado => "Recusado",
            Self::Expirado => "Expirado",
            Self::Cancelado => "Cancelado",
            Self::Convertido => "Convertido em OS",
        }
    }

    /// Verdadeiro enquanto o cabeçalho e os itens ainda podem ser editados.
    #[must_use]
    pub const fn aceita_edicao(self) -> bool {
        matches!(self, Self::Rascunho | Self::Enviado)
    }
}

/// Um orçamento comercial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orcamento {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Sequencial por empresa.
    pub numero: u64,
    /// O cliente cadastrado, quando não é avulso.
    pub cliente: Option<Id>,
    /// Nome do cliente — sempre presente (snapshot, mesmo com cliente cadastrado).
    pub cliente_nome: String,
    /// Documento do cliente (snapshot).
    pub cliente_documento: Option<String>,
    /// Contato do cliente — telefone/e-mail (snapshot).
    pub cliente_contato: Option<String>,
    /// Assunto / título do trabalho.
    pub assunto: String,
    /// Texto de apresentação / escopo.
    pub descricao: Option<String>,
    /// Data de emissão.
    pub data_emissao: Data,
    /// Validade da proposta.
    pub validade: Data,
    /// Condições de pagamento.
    pub condicoes_pagamento: Option<String>,
    /// Prazo de entrega / execução.
    pub prazo_entrega: Option<String>,
    /// Observações gerais.
    pub observacoes: Option<String>,
    /// Desconto de cabeçalho, aplicado sobre a soma líquida dos itens.
    pub desconto_percentual: Percentual,
    /// O estado.
    pub estado: EstadoOrcamento,
    /// Quem emitiu.
    pub responsavel: Id,
    /// Nome/documento de quem aprovou — nunca implícito.
    pub aprovado_por: Option<String>,
    /// A OS gerada, quando convertido.
    pub os_gerada: Option<Id>,
    /// Quando foi criado.
    pub criado_em: Instante,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

/// Os campos de cabeçalho de um orçamento — o que `novo` e `EditarOrcamento` recebem.
#[derive(Debug, Clone)]
pub struct Cabecalho {
    /// O cliente cadastrado (ou `None` para avulso).
    pub cliente: Option<Id>,
    /// Nome do cliente.
    pub cliente_nome: String,
    /// Documento do cliente.
    pub cliente_documento: Option<String>,
    /// Contato do cliente.
    pub cliente_contato: Option<String>,
    /// Assunto.
    pub assunto: String,
    /// Descrição / escopo.
    pub descricao: Option<String>,
    /// Validade.
    pub validade: Data,
    /// Condições de pagamento.
    pub condicoes_pagamento: Option<String>,
    /// Prazo de entrega.
    pub prazo_entrega: Option<String>,
    /// Observações.
    pub observacoes: Option<String>,
    /// Desconto de cabeçalho.
    pub desconto_percentual: Percentual,
}

fn limpar(s: Option<String>) -> Option<String> {
    s.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

impl Orcamento {
    /// Cria um orçamento em `Rascunho`.
    ///
    /// # Errors
    /// [`ErroOrcamentos::AssuntoVazio`], [`ErroOrcamentos::ClienteSemNome`],
    /// [`ErroOrcamentos::ValidadeAnteriorAEmissao`].
    pub fn novo(
        empresa: Id,
        numero: u64,
        cab: Cabecalho,
        data_emissao: Data,
        responsavel: Id,
        criado_em: Instante,
    ) -> Result<Self, ErroOrcamentos> {
        let assunto = cab.assunto.trim().to_owned();
        if assunto.is_empty() {
            return Err(ErroOrcamentos::AssuntoVazio);
        }
        let cliente_nome = cab.cliente_nome.trim().to_owned();
        if cliente_nome.is_empty() {
            return Err(ErroOrcamentos::ClienteSemNome);
        }
        if cab.validade < data_emissao {
            return Err(ErroOrcamentos::ValidadeAnteriorAEmissao);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            numero,
            cliente: cab.cliente,
            cliente_nome,
            cliente_documento: limpar(cab.cliente_documento),
            cliente_contato: limpar(cab.cliente_contato),
            assunto,
            descricao: limpar(cab.descricao),
            data_emissao,
            validade: cab.validade,
            condicoes_pagamento: limpar(cab.condicoes_pagamento),
            prazo_entrega: limpar(cab.prazo_entrega),
            observacoes: limpar(cab.observacoes),
            desconto_percentual: cab.desconto_percentual,
            estado: EstadoOrcamento::Rascunho,
            responsavel,
            aprovado_por: None,
            os_gerada: None,
            criado_em,
            versao: Versao::INICIAL,
        })
    }

    /// Regrava o cabeçalho — só enquanto [`EstadoOrcamento::aceita_edicao`].
    ///
    /// # Errors
    /// [`ErroOrcamentos::EstadoInvalido`] e as mesmas validações de [`Self::novo`].
    pub fn editar_cabecalho(&mut self, cab: Cabecalho) -> Result<(), ErroOrcamentos> {
        self.exigir_edicao()?;
        let assunto = cab.assunto.trim().to_owned();
        if assunto.is_empty() {
            return Err(ErroOrcamentos::AssuntoVazio);
        }
        let cliente_nome = cab.cliente_nome.trim().to_owned();
        if cliente_nome.is_empty() {
            return Err(ErroOrcamentos::ClienteSemNome);
        }
        if cab.validade < self.data_emissao {
            return Err(ErroOrcamentos::ValidadeAnteriorAEmissao);
        }
        self.cliente = cab.cliente;
        self.cliente_nome = cliente_nome;
        self.cliente_documento = limpar(cab.cliente_documento);
        self.cliente_contato = limpar(cab.cliente_contato);
        self.assunto = assunto;
        self.descricao = limpar(cab.descricao);
        self.validade = cab.validade;
        self.condicoes_pagamento = limpar(cab.condicoes_pagamento);
        self.prazo_entrega = limpar(cab.prazo_entrega);
        self.observacoes = limpar(cab.observacoes);
        self.desconto_percentual = cab.desconto_percentual;
        self.versao = self.versao.proxima();
        Ok(())
    }

    // ── Totais ──────────────────────────────────────────────────────────────

    /// Soma dos itens antes de qualquer desconto.
    #[must_use]
    pub fn subtotal(itens: &[ItemOrcamento]) -> Dinheiro {
        itens.iter().map(ItemOrcamento::bruto).sum()
    }

    /// Soma dos descontos de item + o desconto de cabeçalho.
    #[must_use]
    pub fn desconto_total(&self, itens: &[ItemOrcamento]) -> Dinheiro {
        let desconto_itens: Dinheiro = itens.iter().map(ItemOrcamento::desconto_valor).sum();
        desconto_itens + self.desconto_cabecalho(itens)
    }

    /// O total líquido.
    #[must_use]
    pub fn total(&self, itens: &[ItemOrcamento]) -> Dinheiro {
        let liquido_itens: Dinheiro = itens.iter().map(|i| i.total).sum();
        liquido_itens - self.desconto_cabecalho(itens)
    }

    fn desconto_cabecalho(&self, itens: &[ItemOrcamento]) -> Dinheiro {
        let liquido_itens: Dinheiro = itens.iter().map(|i| i.total).sum();
        liquido_itens.aplicar(self.desconto_percentual, Arredondamento::MeioAcima)
    }

    // ── Ciclo de vida ───────────────────────────────────────────────────────

    /// `Rascunho → Enviado`.
    ///
    /// # Errors
    /// [`ErroOrcamentos::EstadoInvalido`] fora de `Rascunho`;
    /// [`ErroOrcamentos::OrcamentoVazio`] sem item; [`ErroOrcamentos::OrcamentoVencido`].
    pub fn enviar(&mut self, quantidade_itens: usize, hoje: Data) -> Result<(), ErroOrcamentos> {
        if self.estado != EstadoOrcamento::Rascunho {
            return Err(ErroOrcamentos::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Rascunho",
            });
        }
        if quantidade_itens == 0 {
            return Err(ErroOrcamentos::OrcamentoVazio);
        }
        if self.validade < hoje {
            return Err(ErroOrcamentos::OrcamentoVencido(self.validade.formatar()));
        }
        self.transitar(EstadoOrcamento::Enviado);
        Ok(())
    }

    /// `Enviado → Aprovado | Recusado`. `identificacao` é obrigatória para aprovar.
    ///
    /// # Errors
    /// [`ErroOrcamentos::OrcamentoJaDecidido`], [`ErroOrcamentos::OrcamentoVencido`],
    /// [`ErroOrcamentos::DecisaoSemIdentificacao`].
    pub fn registrar_decisao(
        &mut self,
        aprovado: bool,
        identificacao: Option<String>,
        hoje: Data,
    ) -> Result<(), ErroOrcamentos> {
        if self.estado != EstadoOrcamento::Enviado {
            return Err(ErroOrcamentos::OrcamentoJaDecidido);
        }
        if aprovado {
            if self.validade < hoje {
                return Err(ErroOrcamentos::OrcamentoVencido(self.validade.formatar()));
            }
            let ident = identificacao
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .ok_or(ErroOrcamentos::DecisaoSemIdentificacao)?;
            self.aprovado_por = Some(ident);
            self.transitar(EstadoOrcamento::Aprovado);
        } else {
            self.transitar(EstadoOrcamento::Recusado);
        }
        Ok(())
    }

    /// Marca `Rascunho`/`Enviado` vencido como `Expirado`. No-op caso contrário.
    pub fn expirar_se_vencido(&mut self, hoje: Data) -> bool {
        if matches!(self.estado, EstadoOrcamento::Rascunho | EstadoOrcamento::Enviado)
            && self.validade < hoje
        {
            self.transitar(EstadoOrcamento::Expirado);
            true
        } else {
            false
        }
    }

    /// Cancela o orçamento (a partir de `Rascunho`/`Enviado`/`Aprovado`).
    ///
    /// # Errors
    /// [`ErroOrcamentos::EstadoInvalido`] de um estado terminal.
    pub fn cancelar(&mut self) -> Result<(), ErroOrcamentos> {
        if !matches!(
            self.estado,
            EstadoOrcamento::Rascunho | EstadoOrcamento::Enviado | EstadoOrcamento::Aprovado
        ) {
            return Err(ErroOrcamentos::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Rascunho, Enviado ou Aprovado",
            });
        }
        self.transitar(EstadoOrcamento::Cancelado);
        Ok(())
    }

    /// `Aprovado → Convertido`, guardando a OS gerada.
    ///
    /// # Errors
    /// [`ErroOrcamentos::EstadoInvalido`] fora de `Aprovado`.
    pub fn marcar_convertido(&mut self, ordem_servico: Id) -> Result<(), ErroOrcamentos> {
        if self.estado != EstadoOrcamento::Aprovado {
            return Err(ErroOrcamentos::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aprovado",
            });
        }
        self.os_gerada = Some(ordem_servico);
        self.transitar(EstadoOrcamento::Convertido);
        Ok(())
    }

    /// Sinaliza que os itens mudaram (só bump de versão) — usado por `DefinirItensOrcamento`.
    ///
    /// # Errors
    /// [`ErroOrcamentos::EstadoInvalido`] se o estado não aceita mais edição.
    pub fn itens_alterados(&mut self) -> Result<(), ErroOrcamentos> {
        self.exigir_edicao()?;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Verdadeiro se o estado ainda aceita editar cabeçalho/itens; erro caso contrário.
    ///
    /// # Errors
    /// [`ErroOrcamentos::EstadoInvalido`] se o estado não aceita mais edição.
    pub fn exigir_edicao(&self) -> Result<(), ErroOrcamentos> {
        if self.estado.aceita_edicao() {
            Ok(())
        } else {
            Err(ErroOrcamentos::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Rascunho ou Enviado",
            })
        }
    }

    fn transitar(&mut self, novo: EstadoOrcamento) {
        self.estado = novo;
        self.versao = self.versao.proxima();
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Preco, Quantidade};

    use super::*;
    use crate::item::ItemOrcamento;

    fn hoje() -> Data {
        Data::de_dias(20_000)
    }

    fn cabecalho() -> Cabecalho {
        Cabecalho {
            cliente: None,
            cliente_nome: "Maria Souza".to_owned(),
            cliente_documento: None,
            cliente_contato: None,
            assunto: "Reforma do motor".to_owned(),
            descricao: None,
            validade: hoje().mais_dias(15),
            condicoes_pagamento: None,
            prazo_entrega: None,
            observacoes: None,
            desconto_percentual: Percentual::ZERO,
        }
    }

    fn orcamento() -> Orcamento {
        Orcamento::novo(Id::novo(), 1, cabecalho(), hoje(), Id::novo(), Instante::EPOCA).unwrap()
    }

    fn item(o: Id) -> ItemOrcamento {
        ItemOrcamento::novo(
            o,
            0,
            "Serviço",
            Quantidade::unidades(1),
            "un",
            Preco::reais(100),
            Percentual::ZERO,
        )
        .unwrap()
    }

    #[test]
    fn assunto_e_cliente_obrigatorios() {
        let mut c = cabecalho();
        c.assunto = "   ".to_owned();
        assert_eq!(
            Orcamento::novo(Id::novo(), 1, c, hoje(), Id::novo(), Instante::EPOCA).unwrap_err(),
            ErroOrcamentos::AssuntoVazio
        );
    }

    #[test]
    fn fluxo_feliz() {
        let mut o = orcamento();
        let itens = vec![item(o.id)];
        assert_eq!(o.total(&itens), Dinheiro::reais(100));

        o.enviar(itens.len(), hoje()).unwrap();
        assert_eq!(o.estado, EstadoOrcamento::Enviado);

        o.registrar_decisao(true, Some("Maria Souza - CPF".to_owned()), hoje())
            .unwrap();
        assert_eq!(o.estado, EstadoOrcamento::Aprovado);

        let os = Id::novo();
        o.marcar_convertido(os).unwrap();
        assert_eq!(o.estado, EstadoOrcamento::Convertido);
        assert_eq!(o.os_gerada, Some(os));
    }

    #[test]
    fn enviar_sem_item_e_recusado() {
        let mut o = orcamento();
        assert_eq!(o.enviar(0, hoje()).unwrap_err(), ErroOrcamentos::OrcamentoVazio);
    }

    #[test]
    fn aprovar_sem_identificacao_e_recusado() {
        let mut o = orcamento();
        o.enviar(1, hoje()).unwrap();
        assert_eq!(
            o.registrar_decisao(true, None, hoje()).unwrap_err(),
            ErroOrcamentos::DecisaoSemIdentificacao
        );
    }

    #[test]
    fn vencido_expira_e_nao_aprova() {
        let mut o = orcamento();
        o.enviar(1, hoje()).unwrap();
        let depois = o.validade.mais_dias(1);
        assert!(matches!(
            o.registrar_decisao(true, Some("x".to_owned()), depois).unwrap_err(),
            ErroOrcamentos::OrcamentoVencido(_)
        ));
        assert!(o.expirar_se_vencido(depois));
        assert_eq!(o.estado, EstadoOrcamento::Expirado);
    }

    #[test]
    fn desconto_de_cabecalho_incide_sobre_o_liquido() {
        let mut o = orcamento();
        o.desconto_percentual = Percentual::pontos(10);
        let itens = vec![item(o.id)];
        assert_eq!(o.total(&itens), Dinheiro::reais(90));
        assert_eq!(o.desconto_total(&itens), Dinheiro::reais(10));
    }
}
