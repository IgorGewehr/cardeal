//! Gera uma planilha de amostra (`cargo run -p cardeal-planilha --example amostra -- saida.xlsx`)
//! para conferir num Excel/LibreOffice de verdade.

use cardeal_kernel::{Data, Dinheiro};
use cardeal_planilha::{Aba, Celula, Planilha};

fn main() {
    let caminho = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "amostra.xlsx".to_owned());
    let mut dre = Aba::nova("DRE").colunas(&[40.0, 16.0]).congelar_cabecalho();
    dre.linha(vec![
        Celula::texto("Linha").negrito(),
        Celula::texto("Valor").negrito(),
    ]);
    dre.linha(vec![
        Celula::texto("Receita bruta"),
        Celula::dinheiro(Dinheiro::centavos(1_234_567)),
    ]);
    dre.linha(vec![
        Celula::texto("Lucro líquido").negrito(),
        Celula::dinheiro(Dinheiro::centavos(-9_999)).negrito(),
    ]);
    let mut diario = Aba::nova("Diário");
    diario.linha(vec![
        Celula::data(Data::de_ymd(2026, 10, 14).unwrap()),
        Celula::inteiro(42),
        Celula::texto("Venda & <ação>"),
    ]);
    std::fs::write(&caminho, Planilha::nova().aba(dre).aba(diario).gerar()).unwrap();
}
