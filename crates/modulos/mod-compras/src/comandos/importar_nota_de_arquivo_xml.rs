//! Importa uma nota de entrada a partir do texto de um XML de NF-e já em mãos — lido de um
//! arquivo local pela UI (via `rfd`, escolha de arquivo do sistema), não baixado da
//! distribuição `DFe`. O pedido do dono do produto era literal: "importação de notas de
//! compra reto pro estoque" a partir de um arquivo, sem depender da SEFAZ estar disponível
//! (XML que o fornecedor mandou por e-mail, nota de uma filial sem certificado configurado
//! ainda, XML salvo de outro sistema).
//!
//! Ao contrário de [`super::importar_nota_da_sefaz`]/[`super::verificar_notas_na_sefaz`],
//! este **é** um `Comando` de despacho de verdade: não depende de `PortaFiscal` nenhum, só
//! interpreta o texto do XML (`cardeal_fiscal::interpretar`, puro, sem rede) e roda o mesmo
//! miolo comum ([`super::importar_nota_interpretada`]) que a importação por SEFAZ usa —
//! mesma resolução de fornecedor por CNPJ, mesma cascata de casamento (GTIN → regra
//! aprendida → NCM+similaridade), mesmo rateio de despesas, mesma idempotência por
//! `(empresa, chave_acesso)` (reimportar o mesmo arquivo não duplica nada — devolve o
//! relatório da nota que já existe).
//!
//! A leitura do arquivo em si (diálogo `rfd`, decodificação de encoding) é responsabilidade
//! da UI — este comando só recebe o texto já em UTF-8.

use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::{importar_nota_interpretada, RelatorioImportacao};

/// Importa uma nota de entrada a partir do XML de uma NF-e já lido de um arquivo local.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarNotaDeArquivoXml {
    /// O texto do XML (UTF-8), já lido do arquivo pela UI.
    pub xml: String,
}

impl Comando for ImportarNotaDeArquivoXml {
    type Saida = RelatorioImportacao;
    const PERMISSAO: &'static str = "compras.entrada.importar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let nota_xml = cardeal_fiscal::interpretar(&self.xml).map_err(|e| Erro::de_dominio(&e))?;
        importar_nota_interpretada(&nota_xml, ctx, uow)
    }
}
