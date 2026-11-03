//! A **verificação em duas etapas** do usuário local (`cardeal_auth::totp`): ativar (o segredo
//! vai ao aplicativo pelo QR Code e o primeiro código o confirma), conferir no login, desativar
//! e o administrador desligar a de quem perdeu o celular.
//!
//! O estado inteiro (o segredo, os resumos dos códigos de recuperação e o último código aceito)
//! vai **selado pelo cofre** (`cardeal_storage::Cofre`, AES-256-GCM, chave fora da base) na
//! coluna `nucleo_usuario.mfa_segredo`, amarrado ao usuário: quem copia o `.db` não leva nada.

pub use cardeal_auth::totp::SegundoFatorProposto;
use cardeal_auth::totp::{Aceito, SegredoTotp, SegundoFator};
use cardeal_auth::{RepositorioAuth, Usuario};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use cardeal_modkit::Recursos;
use cardeal_storage::{Armazenamento, Cofre, ContextoEscrita};

use crate::{como_erro_armazenamento, erro_armazenamento};

/// O emissor que o aplicativo autenticador mostra.
pub const EMISSOR: &str = "Cardeal";

fn cofre(recursos: &Recursos) -> Resultado<std::sync::Arc<Cofre>> {
    recursos.obter::<Cofre>().ok_or_else(|| {
        Erro::novo(
            CodigoErro::FALHA_INTERNA,
            "o cofre de segredos não está disponível nesta instalação",
        )
    })
}

/// O estado do segundo fator, aberto do cofre. `Ok(None)`: sem verificação em duas etapas.
fn ler(arm: &Armazenamento, recursos: &Recursos, usuario: Id) -> Resultado<Option<SegundoFator>> {
    let Some(selado) = arm
        .leitor()
        .consultar(move |c| {
            cardeal_auth::consultas::segundo_fator_selado(c, usuario)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?
    else {
        return Ok(None);
    };
    cardeal_auth::totp::abrir(&selado, &*cofre(recursos)?, &usuario.em_bytes()).map(Some)
}

/// Grava (selado) ou apaga o estado do segundo fator.
fn gravar(
    arm: &Armazenamento,
    recursos: &Recursos,
    empresa: Id,
    usuario: Id,
    estado: Option<&SegundoFator>,
) -> Resultado<()> {
    let selado = match estado {
        Some(e) => Some(cardeal_auth::totp::selar(
            e,
            &*cofre(recursos)?,
            &usuario.em_bytes(),
        )),
        None => None,
    };
    let ctx = ContextoEscrita::novo(empresa, usuario, Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            RepositorioAuth::novo(uow)
                .gravar_segundo_fator(usuario, selado.as_deref())
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)
        .map(|_| ())
}

/// A pessoa tem verificação em duas etapas?
///
/// # Errors
/// Erro de leitura.
pub(crate) fn ativo(arm: &Armazenamento, usuario: Id) -> Resultado<bool> {
    arm.leitor()
        .consultar(move |c| {
            cardeal_auth::consultas::segundo_fator_selado(c, usuario)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map(|s| s.is_some())
        .map_err(erro_armazenamento)
}

/// Um segredo novo para a pessoa levar ao aplicativo (nada é gravado ainda).
#[must_use]
pub(crate) fn propor(login: &str) -> SegundoFatorProposto {
    let segredo = SegredoTotp::gerar();
    SegundoFatorProposto {
        uri: segredo.uri(EMISSOR, login),
        segredo: segredo.em_base32(),
    }
}

/// Ativa: o código do aplicativo confirma que o segredo foi lido. Devolve os códigos de
/// recuperação, em claro, para mostrar **uma vez**.
///
/// # Errors
/// `ENTRADA_INVALIDA` (segredo ou código que não confere); `REGRA_VIOLADA` (já ativa).
pub(crate) fn ativar(
    arm: &Armazenamento,
    recursos: &Recursos,
    empresa: Id,
    usuario: Id,
    segredo: &str,
    codigo: &str,
) -> Resultado<Vec<String>> {
    if ativo(arm, usuario)? {
        return Err(Erro::novo(
            CodigoErro::REGRA_VIOLADA,
            "A verificação em duas etapas já está ativa — desative antes de trocar o aplicativo",
        ));
    }
    let segredo = SegredoTotp::de_base32(segredo)
        .ok_or_else(|| Erro::novo(CodigoErro::ENTRADA_INVALIDA, "Segredo inválido"))?;
    let (mut estado, codigos) = SegundoFator::novo(segredo, &usuario.em_bytes());
    let passo = estado
        .segredo
        .conferir(codigo, Instante::agora(), None)
        .ok_or_else(|| {
            Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "O código não confere — confira se o relógio do celular está certo e digite o \
                 código que aparece agora",
            )
            .no_campo("codigo")
        })?;
    estado.ultimo_passo = Some(passo);
    gravar(arm, recursos, empresa, usuario, Some(&estado))?;
    Ok(codigos)
}

/// Desativa a da própria pessoa: a senha e um código (do aplicativo ou de recuperação).
///
/// # Errors
/// `CREDENCIAL_INVALIDA` (senha ou código); `REGRA_VIOLADA` (não estava ativa).
pub(crate) fn desativar(
    arm: &Armazenamento,
    recursos: &Recursos,
    empresa: Id,
    usuario: &Usuario,
    senha: &str,
    codigo: &str,
) -> Resultado<()> {
    if !cardeal_auth::verificar_senha(senha, &usuario.hash_senha) {
        return Err(Erro::novo(
            CodigoErro::CREDENCIAL_INVALIDA,
            "A senha não confere",
        ));
    }
    let mut estado = ler(arm, recursos, usuario.id)?.ok_or_else(|| {
        Erro::novo(
            CodigoErro::REGRA_VIOLADA,
            "A verificação em duas etapas não está ativa",
        )
    })?;
    if estado
        .conferir(codigo, Instante::agora(), &usuario.id.em_bytes())
        .is_none()
    {
        return Err(Erro::novo(
            CodigoErro::CREDENCIAL_INVALIDA,
            "Código inválido",
        ));
    }
    gravar(arm, recursos, empresa, usuario.id, None)
}

/// O administrador desliga a de outra pessoa (perdeu o celular e os códigos de recuperação).
///
/// # Errors
/// Erro de escrita.
pub(crate) fn desligar(
    arm: &Armazenamento,
    recursos: &Recursos,
    empresa: Id,
    usuario: Id,
) -> Resultado<()> {
    gravar(arm, recursos, empresa, usuario, None)
}

/// No login, depois da senha: confere o código e grava o estado novo (o último passo, ou o
/// código de recuperação gasto). `Ok(None)`: a pessoa não tem verificação em duas etapas.
/// `Ok(Some(false))`: o código não confere.
///
/// # Errors
/// `SEGUNDO_FATOR_NECESSARIO` sem código; erro do cofre ou de escrita.
pub(crate) fn conferir_no_login(
    arm: &Armazenamento,
    recursos: &Recursos,
    empresa: Id,
    usuario: Id,
    codigo: Option<&str>,
) -> Resultado<Option<bool>> {
    let Some(mut estado) = ler(arm, recursos, usuario)? else {
        return Ok(None);
    };
    let Some(codigo) = codigo.filter(|c| !c.trim().is_empty()) else {
        return Err(Erro::novo(
            CodigoErro::SEGUNDO_FATOR_NECESSARIO,
            "Digite o código do aplicativo autenticador",
        )
        .no_campo("codigo"));
    };
    match estado.conferir(codigo, Instante::agora(), &usuario.em_bytes()) {
        Some(aceito) => {
            gravar(arm, recursos, empresa, usuario, Some(&estado))?;
            if let Aceito::Recuperacao { restantes } = aceito {
                tracing::warn!(
                    restantes,
                    "login com código de recuperação da verificação em duas etapas"
                );
            }
            Ok(Some(true))
        }
        None => Ok(Some(false)),
    }
}
