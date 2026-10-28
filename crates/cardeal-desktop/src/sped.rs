//! A EFD ICMS/IPI (SPED Fiscal) de um mês, montada pelo cliente: as notas de compra que
//! entraram vêm do `compras` (com o produto do cadastro de cada item) e o arquivo sai do
//! `fiscal`, que tem as notas emitidas — o `fiscal` não enxerga as compras. Usado pelo cartão
//! do SPED (Configurações › Fiscal) e pelo pacote do contador.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Competencia, Dinheiro};
use mod_compras::escrituracao::{EntradaParaEscriturar, EntradasDoMes, EntradasParaEscriturar};
use mod_fiscal::efd::{EfdDoMes, EntradaEscriturada, GerarEfd, ItemDoCadastro};

/// As entradas do `compras` no formato do `fiscal` (o item com o código que vai nas notas).
fn converter(entradas: Vec<EntradaParaEscriturar>) -> Vec<EntradaEscriturada> {
    entradas
        .into_iter()
        .map(|e| EntradaEscriturada {
            xml: e.xml,
            data_entrada: e.data_entrada,
            itens: e
                .itens
                .into_iter()
                .map(|p| {
                    p.map(|p| ItemDoCadastro {
                        codigo: mod_fiscal::codigo_do_produto(p.produto),
                        descricao: p.descricao,
                        unidade: p.unidade,
                        ncm: p.ncm,
                        gtin: p.gtin,
                        cest: p.cest,
                    })
                })
                .collect(),
        })
        .collect()
}

/// Gera a EFD do mês. Sem acesso às compras, o arquivo sai só com as notas emitidas e um aviso.
///
/// # Errors
/// O erro da consulta do `fiscal` (sem permissão, motor fora).
pub fn gerar(
    motor: &Motor,
    sessao: &Sessao,
    mes: Competencia,
    substituta: bool,
    saldo_credor_anterior: Option<Dinheiro>,
) -> Result<EfdDoMes, String> {
    let mut avisos = Vec::new();
    let entradas = if sessao.concede("compras.entrada.ver") {
        let r: Result<EntradasDoMes, _> = motor.consultar(
            sessao,
            "compras.entradas_para_escriturar.v1",
            &EntradasParaEscriturar { competencia: mes },
        );
        match r {
            Ok(e) => {
                avisos.extend(e.sem_xml.iter().map(|n| {
                    format!("Compra {n} sem o XML guardado — importe o XML de novo; ficou fora.")
                }));
                converter(e.entradas)
            }
            Err(e) => {
                avisos.push(format!("As compras ficaram fora: {}", e.mensagem));
                Vec::new()
            }
        }
    } else {
        avisos.push("O seu usuário não vê as compras: as entradas ficaram fora.".to_owned());
        Vec::new()
    };
    let mut efd: EfdDoMes = motor
        .consultar(
            sessao,
            "fiscal.gerar_efd.v1",
            &GerarEfd {
                competencia: mes,
                substituta,
                entradas,
                saldo_credor_anterior,
            },
        )
        .map_err(|e| e.mensagem)?;
    if let Some(a) = &mut efd.arquivo {
        a.avisos.extend(avisos);
    }
    Ok(efd)
}

/// Se o impedimento é só o regime (o Simples não entrega a EFD ICMS/IPI).
#[must_use]
pub fn dispensada(efd: &EfdDoMes) -> bool {
    efd.arquivo.is_none()
        && efd
            .impedimentos
            .iter()
            .any(|i| i == mod_fiscal::efd::EFD_DISPENSADA)
}
