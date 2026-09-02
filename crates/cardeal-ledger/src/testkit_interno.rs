//! Implementação de [`PortaRazao`] em memória, só para os testes deste crate.
//!
//! Quando `cardeal-storage` existir, o mesmo papel será cumprido pela `UnidadeDeTrabalho`
//! real, sobre SQLite — mas os testes que usam `RazaoEmMemoria` continuam válidos sem
//! alteração, porque dependem só da trait. Ver `docs/contratos-internos.md` §6.

#![allow(dead_code)]

use std::collections::HashMap;

use cardeal_kernel::{Data, Id, Instante};

use crate::conta::{PapelConta, TipoConta};
use crate::lancamento::Lancamento;
use crate::plano::plano_padrao;
use crate::porta::{InfoConta, PortaRazao};

pub struct RazaoEmMemoria {
    usuario: Id,
    dispositivo: Id,
    agora: Instante,
    contas: HashMap<Id, InfoConta>,
    por_papel: HashMap<(Id, PapelConta), Id>,
    por_codigo: HashMap<(Id, String), Id>,
    periodo_fechado: HashMap<Id, Data>,
    proximo_numero: HashMap<Id, u64>,
    lancamentos: HashMap<Id, Lancamento>,
}

impl RazaoEmMemoria {
    pub fn nova() -> Self {
        Self {
            usuario: Id::novo(),
            dispositivo: Id::novo(),
            agora: Instante::agora(),
            contas: HashMap::new(),
            por_papel: HashMap::new(),
            por_codigo: HashMap::new(),
            periodo_fechado: HashMap::new(),
            proximo_numero: HashMap::new(),
            lancamentos: HashMap::new(),
        }
    }

    /// Insere uma conta avulsa e devolve o `Id` gerado.
    pub fn inserir_conta(
        &mut self,
        empresa: Id,
        codigo: &str,
        tipo: TipoConta,
        ativa: bool,
        papel: Option<PapelConta>,
    ) -> Id {
        let id = Id::novo();
        self.contas.insert(
            id,
            InfoConta {
                empresa,
                codigo: codigo.to_string(),
                tipo,
                ativa,
            },
        );
        self.por_codigo.insert((empresa, codigo.to_string()), id);
        if let Some(p) = papel {
            self.por_papel.insert((empresa, p), id);
        }
        id
    }

    /// Semeia o plano de contas padrão inteiro para a empresa e devolve o `Id` da conta
    /// Caixa — é a que a maioria dos cenários de teste usa primeiro.
    pub fn semear_plano_padrao(&mut self, empresa: Id) -> Id {
        let mut caixa = None;
        for semente in plano_padrao() {
            let id = self.inserir_conta(
                empresa,
                semente.codigo.como_str(),
                semente.tipo,
                true,
                semente.papel,
            );
            if semente.papel == Some(PapelConta::Caixa) {
                caixa = Some(id);
            }
        }
        caixa.expect("o plano padrão sempre inclui a conta Caixa")
    }

    /// Marca a empresa com período fechado até a data informada.
    pub fn fechar_periodo(&mut self, empresa: Id, ate: Data) {
        self.periodo_fechado.insert(empresa, ate);
    }

    /// Remove o fechamento de período da empresa.
    pub fn reabrir_periodo(&mut self, empresa: Id) {
        self.periodo_fechado.remove(&empresa);
    }
}

impl PortaRazao for RazaoEmMemoria {
    fn usuario(&self) -> Id {
        self.usuario
    }

    fn dispositivo(&self) -> Id {
        self.dispositivo
    }

    fn agora(&self) -> Instante {
        self.agora
    }

    fn info_conta(&self, conta: Id) -> Option<InfoConta> {
        self.contas.get(&conta).cloned()
    }

    fn conta_por_papel(&self, empresa: Id, papel: PapelConta) -> Option<Id> {
        self.por_papel.get(&(empresa, papel)).copied()
    }

    fn conta_por_codigo(&self, empresa: Id, codigo: &str) -> Option<Id> {
        self.por_codigo.get(&(empresa, codigo.to_string())).copied()
    }

    fn periodo_fechado_ate(&self, empresa: Id) -> Option<Data> {
        self.periodo_fechado.get(&empresa).copied()
    }

    fn proximo_numero_lancamento(&mut self, empresa: Id) -> u64 {
        let contador = self.proximo_numero.entry(empresa).or_insert(0);
        *contador += 1;
        *contador
    }

    fn inserir_lancamento(&mut self, lancamento: Lancamento) {
        self.lancamentos.insert(lancamento.id, lancamento);
    }

    fn buscar_lancamento(&self, id: Id) -> Option<Lancamento> {
        self.lancamentos.get(&id).cloned()
    }

    fn atualizar_lancamento(&mut self, lancamento: Lancamento) {
        self.lancamentos.insert(lancamento.id, lancamento);
    }
}
