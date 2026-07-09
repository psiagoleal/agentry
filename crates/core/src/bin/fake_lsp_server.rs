// Caminho relativo: crates/core/src/bin/fake_lsp_server.rs
//! Fixture de teste (MT-23/24, ADR-0013): um "language server" mínimo, só o
//! suficiente para os testes de ciclo de vida de `LspClient`
//! (`start` → `initialize` → `shutdown`, MT-23) e das tools de leitura
//! (`textDocument/hover`, MT-24) exercitarem spawn + JSON-RPC real sobre
//! stdio, sem depender de nenhum *language server* de verdade instalado no
//! ambiente de CI (ADR-0013 proíbe o `agentry` de empacotar um). Não é
//! parte do produto — só um binário auxiliar de teste, spawnado via
//! `env!("CARGO_BIN_EXE_fake_lsp_server")` a partir dos testes.
//!
//! Responde `initialize`/`shutdown`/`textDocument/hover` com sucesso
//! trivial (fixo, independente da posição/URI pedida) e encerra ao receber
//! a notificação `exit` — mesma técnica de *framing* (`lsp_server::Message`)
//! usada pelo cliente real, só no papel de servidor desta vez.

use std::io::{stdin, stdout, BufReader};

use lsp_server::{Message, Response};

fn main() {
    let mut entrada = BufReader::new(stdin());
    let mut saida = stdout();

    while let Ok(Some(mensagem)) = Message::read(&mut entrada) {
        match mensagem {
            Message::Request(req) if req.method == "initialize" => {
                let resposta = Response::new_ok(req.id, serde_json::json!({ "capabilities": {} }));
                let _ = Message::Response(resposta).write(&mut saida);
            }
            Message::Request(req) if req.method == "shutdown" => {
                let resposta = Response::new_ok(req.id, serde_json::Value::Null);
                let _ = Message::Response(resposta).write(&mut saida);
            }
            Message::Request(req) if req.method == "textDocument/hover" => {
                let resposta = Response::new_ok(
                    req.id,
                    serde_json::json!({
                        "contents": { "kind": "plaintext", "value": "fake hover: Foo -> i32" }
                    }),
                );
                let _ = Message::Response(resposta).write(&mut saida);
            }
            Message::Notification(nota) if nota.method == "exit" => break,
            _ => {} // didOpen e demais notificações: ignoradas nesta fixture mínima
        }
    }
}
