//! # Core de Processamento Fixed-Width (CNAB)
//!
//! Este módulo fornece as estruturas e funções fundamentais para parsear linhas de texto
//! com largura fixa (Fixed Width), comum em arquivos bancários (CNAB 240/400).
//!
//! O foco deste core é a **extração segura e tipada** dos dados, delegando validações
//! de negócio (CPF, datas, lógica de banco) para a camada superior.
pub use cnab_derive::FixedWidth;

use std::collections::HashMap;
use std::ops::Range;
use thiserror::Error;

/// Define a posição de um campo conforme manuais bancários (CNAB).
///
/// # Importante
/// Manuais de banco utilizam indexação **baseada em 1** e **inclusiva**.
/// Exemplo: "Posição 001 a 003" significa os 3 primeiros caracteres.
#[derive(Debug, Clone, Copy)]
pub struct FieldPos {
    /// Posição inicial (1-based, inclusivo).
    pub start: usize,
    /// Posição final (1-based, inclusivo).
    pub end: usize,
}

impl FieldPos {
    /// Converte a posição CNAB (1-based inclusivo) para um Range Rust (0-based exclusivo).
    ///
    /// Exemplo: CNAB `1..3` (3 chars) torna-se Rust `0..3` (índices 0, 1, 2).
    fn as_range(&self) -> Range<usize> {
        (self.start - 1)..self.end
    }

    /// Retorna a largura total do campo em caracteres.
    pub fn width(&self) -> usize {
        self.end - self.start + 1
    }
}

/// Define o tipo de dado esperado no campo para conversão.
#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    /// Texto alfanumérico.
    /// Geralmente alinhado à esquerda e preenchido com espaços à direita.
    Alpha,

    /// Numérico inteiro.
    /// Geralmente alinhado à direita e preenchido com zeros à esquerda.
    /// Se o campo estiver vazio ou só espaços, será convertido para 0.
    Numeric,

    /// Numérico com casas decimais implícitas.
    ///
    /// Exemplo: A string "000000001234" com `scale: 2` representa `12.34`.
    Decimal {
        /// Número de casas decimais a considerar.
        scale: u8
    },
}

/// Metadados que definem um campo no layout.
///
/// Esta estrutura é geralmente construída automaticamente pela macro derive.
#[derive(Debug, Clone)]
pub struct FieldSpec {
    /// Nome do campo (deve coincidir com o nome na struct alvo).
    /// Usamos `&'static str` para performance (zero alocação na definição).
    pub name: &'static str,

    /// Posição no arquivo.
    pub pos: FieldPos,

    /// Tipo de dado para tratamento.
    pub kind: FieldKind,
}

/// Representação intermediária de um valor parseado.
///
/// O parser extrai a string bruta e converte para uma destas variantes
/// antes de popular a struct final.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Valor textual (String owned).
    Alpha(String),
    /// Valor inteiro (i64).
    Numeric(i64),
    /// Valor decimal representado como inteiro bruto + escala.
    /// Ex: 12.34 vira `Decimal { raw: 1234, scale: 2 }`.
    Decimal { raw: i64, scale: u8 },
}

/// Erros possíveis durante o processo de parsing.
#[derive(Debug, Error)]
pub enum FixedWidthError {
    /// A linha fornecida é mais curta do que a posição final exigida por um campo.
    #[error("linha é menor que o necessário: len={len}, precisa de >= {needed}")]
    LineTooShort { len: usize, needed: usize },

    /// O campo foi definido como Numérico/Decimal, mas contém caracteres não numéricos.
    #[error("campo '{field}' contém caracteres inválidos para numérico: '{snippet}'")]
    InvalidNumeric {
        field: &'static str,
        snippet: String,
    },

    /// Erro genérico de UTF-8 (embora `&str` já garanta UTF-8 válido na entrada).
    #[error("erro de UTF-8 na linha")]
    InvalidUtf8,
}

/// Resultado padrão utilizado pelo crate.
pub type Result<T> = std::result::Result<T, FixedWidthError>;

/// Faz o parse de uma linha de texto bruta com base em uma lista de especificações de campos.
///
/// # Argumentos
/// * `line` - A linha bruta do arquivo (pode conter `\r` ou `\n` no final).
/// * `fields` - Lista de especificações (`FieldSpec`) gerada pela macro.
///
/// # Retorno
/// Retorna um `HashMap` onde a chave é o nome do campo e o valor é o `Value` parseado.
pub fn parse_line<'a>(
    line: &'a str,
    fields: &[FieldSpec],
) -> Result<HashMap<&'static str, Value>> {
    // Remove quebras de linha comuns em Windows (\r\n) e Unix (\n)
    // para evitar que contem no tamanho da string ou sujem o último campo.
    let line = line.trim_end_matches(&['\r', '\n'][..]);
    let len = line.len();

    // Pré-aloca o mapa para evitar realocações dinâmicas
    let mut map = HashMap::with_capacity(fields.len());

    for field in fields {
        // Validação de limites (Bounds check)
        let needed = field.pos.end;
        if len < needed {
            return Err(FixedWidthError::LineTooShort { len, needed });
        }

        // Fatia a string (Slice) usando a conversão segura de índices
        let slice = &line[field.pos.as_range()];

        let value = match field.kind {
            FieldKind::Alpha => {
                // Alpha: Remove espaços à direita (padrão CNAB)
                Value::Alpha(slice.trim_end().to_string())
            }
            FieldKind::Numeric => {
                // Numeric: Remove espaços em volta.
                // Bancos as vezes mandam campos numéricos zerados como espaços em branco.
                let s = slice.trim();
                if s.is_empty() {
                    Value::Numeric(0)
                } else if !s.chars().all(|c| c.is_ascii_digit()) {
                    return Err(FixedWidthError::InvalidNumeric {
                        field: field.name,
                        snippet: slice.to_string(),
                    });
                } else {
                    let n = s.parse::<i64>().map_err(|_| FixedWidthError::InvalidNumeric {
                        field: field.name,
                        snippet: slice.to_string(),
                    })?;
                    Value::Numeric(n)
                }
            }
            FieldKind::Decimal { scale } => {
                // Decimal: Segue a mesma lógica do numérico, mas preserva a escala.
                let s = slice.trim();
                if s.is_empty() {
                    Value::Decimal { raw: 0, scale }
                } else if !s.chars().all(|c| c.is_ascii_digit()) {
                    return Err(FixedWidthError::InvalidNumeric {
                        field: field.name,
                        snippet: slice.to_string(),
                    });
                } else {
                    let n = s.parse::<i64>().map_err(|_| FixedWidthError::InvalidNumeric {
                        field: field.name,
                        snippet: slice.to_string(),
                    })?;
                    Value::Decimal { raw: n, scale }
                }
            }
        };

        map.insert(field.name, value);
    }

    Ok(map)
}

/// Trait implementada automaticamente pela macro derive para expor as especificações dos campos.
pub trait FixedWidthSpec {
    fn spec() -> &'static [FieldSpec];
}

/// Trait principal implementada pela macro derive.
/// Permite instanciar uma Struct a partir de uma linha de texto.
pub trait FixedWidthParse: Sized {
    fn parse(line: &str) -> Result<Self>;
}

// --- Métodos Auxiliares para Value ---

impl Value {
    /// Tenta converter o valor interno para `f64`.
    /// Útil para campos `Decimal`. Aplica a divisão pela potência de 10 conforme a escala.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Decimal { raw, scale } => {
                let factor = 10_i64.pow(*scale as u32) as f64;
                Some(*raw as f64 / factor)
            }
            _ => None,
        }
    }

    /// Tenta converter o valor interno para `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Numeric(n) => Some(*n),
            _ => None,
        }
    }

    /// Tenta obter a referência da string interna (para campos Alpha).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Alpha(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cnab_like_header() {
        // Linha fake com exatamente 240 caracteres para simular CNAB
        let line = "34100000         2297460810001556256000036236       0625610000000362366 MARTINS RIBEIRO ADMINISTRADORABANCO TESTE                              10312202508440000000108501600";

        // Definição manual de campos (o que a macro faria)
        let fields = vec![
            FieldSpec {
                name: "codigo_banco",
                pos: FieldPos { start: 1, end: 3 },
                kind: FieldKind::Numeric,
            },
            FieldSpec {
                name: "lote_servico",
                pos: FieldPos { start: 4, end: 7 },
                kind: FieldKind::Numeric,
            },
            FieldSpec {
                name: "tipo_registro",
                pos: FieldPos { start: 8, end: 8 },
                kind: FieldKind::Numeric,
            },
            FieldSpec {
                name: "nome_banco",
                pos: FieldPos { start: 103, end: 113 },
                kind: FieldKind::Alpha,
            },
        ];

        let parsed = parse_line(&line, &fields).unwrap();

        // Validações
        assert_eq!(parsed["codigo_banco"], Value::Numeric(341));
        assert_eq!(parsed["lote_servico"], Value::Numeric(0));
        assert_eq!(parsed["tipo_registro"], Value::Numeric(0));

        if let Value::Alpha(nome) = &parsed["nome_banco"] {
            // Verifica se o trim funcionou (removeu espaços à direita)
            assert!(nome.starts_with("BANCO TESTE"));
        } else {
            panic!("nome_banco não é Alpha");
        }
    }
}