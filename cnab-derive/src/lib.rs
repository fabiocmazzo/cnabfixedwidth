//! # Derive Macro para FixedWidth
//!
//! Este crate fornece a macro procedural `#[derive(FixedWidth)]` que gera automaticamente
//! a implementação da trait `FixedWidthParse` do crate `cnab_fixedwidth`.
//!
//! # Exemplo de Uso
//!
//! ```ignore
//! use fixedwidth_derive::FixedWidth;
//!
//! #[derive(FixedWidth)]
//! struct Header {
//!     #[fw(pos = "1..3", numeric)]
//!     banco: u32,
//!
//!     #[fw(pos = "4..8", numeric)]
//!     lote: u32,
//!
//!     #[fw(pos = "10..20", alpha)]
//!     texto: String,
//! }
//! ```


use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};

/// Estrutura intermediária para armazenar os dados de um campo
/// extraídos da AST (Abstract Syntax Tree) do código do usuário.
struct ParsedField {
    /// Nome do campo na struct (Identificador).
    ident: syn::Ident,
    /// Tipo do campo (ex: String, i64, f64).
    ty: syn::Type,
    /// Posição inicial (1-based).
    pos_start: usize,
    /// Posição final (1-based).
    pos_end: usize,
    /// Tipo de formatação CNAB (Alpha, Numeric, Decimal).
    kind: FieldKindMacro,
}

/// Representação interna dos tipos de campos suportados pela macro.
enum FieldKindMacro {
    Alpha,
    Numeric,
    Decimal { scale: u8 },
}

/// Helper para parsear a string de posição "start..end".
///
/// Espera o formato "1..10" (inclusive).
/// Retorna erro se o formato for inválido ou se start for 0.
fn parse_pos(lit: &syn::LitStr) -> syn::Result<(usize, usize)> {
    let s = lit.value();
    let parts: Vec<_> = s.split("..").collect();

    if parts.len() != 2 {
        return Err(syn::Error::new_spanned(lit, "pos deve estar no formato start..end"));
    }
    let start = parts[0].parse::<usize>().map_err(|_| syn::Error::new_spanned(lit, "start inválido"))?;
    let end = parts[1].parse::<usize>().map_err(|_| syn::Error::new_spanned(lit, "end inválido"))?;

    if start == 0 || end < start {
        return Err(syn::Error::new_spanned(lit, "pos inválido: start deve ser >=1 e end >= start"));
    }
    Ok((start, end))
}

// --- A MACRO ---

/// Ponto de entrada da Macro Derive.
///
/// Esta função:
/// 1. Lê a struct de entrada.
/// 2. Itera sobre os campos procurando atributos `#[fw(...)]`.
/// 3. Valida se há sobreposição de posições.
/// 4. Gera o código Rust que implementa `FixedWidthParse`.
#[proc_macro_derive(FixedWidth, attributes(fw))]
pub fn derive_fixed_width(input: TokenStream) -> TokenStream {
    // 1. Parse da entrada (Código do usuário)
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Garante que é aplicado apenas em Structs com campos nomeados
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => return syn::Error::new_spanned(&input.ident, "Apenas campos nomeados suportados").to_compile_error().into(),
        },
        _ => return syn::Error::new_spanned(&input.ident, "Apenas structs suportadas").to_compile_error().into(),
    };

    let mut parsed_fields = Vec::new();

    // 2. Extração dos Metadados
    for field in fields {
        let ident = field.ident.clone().unwrap();
        let ty = field.ty.clone();
        let mut pos = None;
        let mut kind = None;

        // Itera sobre os atributos do campo (ex: #[fw(...)])
        for attr in &field.attrs {
            if attr.path().is_ident("fw") {
                attr.parse_nested_meta(|meta| {
                    let name = meta.path.get_ident().map(|i| i.to_string());
                    match name.as_deref() {
                        // Atributo: pos = "1..10"
                        Some("pos") => {
                            let lit: syn::LitStr = meta.value()?.parse()?;
                            pos = Some(parse_pos(&lit)?);
                        }
                        // Atributo: alpha
                        Some("alpha") => kind = Some(FieldKindMacro::Alpha),
                        // Atributo: numeric
                        Some("numeric") => kind = Some(FieldKindMacro::Numeric),
                        // Atributo: decimal = 2
                        Some("decimal") => {
                            let lit: syn::LitInt = meta.value()?.parse()?;
                            kind = Some(FieldKindMacro::Decimal { scale: lit.base10_parse::<u8>()? });
                        }
                        _ => return Err(syn::Error::new_spanned(meta.path, "atributo fw desconhecido")),
                    }
                    Ok(())
                }).expect("parse failed");
            }
        }

        // Valida se os atributos obrigatórios foram preenchidos
        let (start, end) = pos.expect("campo sem pos definido (ex: pos = \"1..10\")");
        let kind = kind.expect("campo sem tipo definido (use alpha, numeric ou decimal)");

        parsed_fields.push(ParsedField { ident, ty, pos_start: start, pos_end: end, kind });
    }

    // 3. Validação de Sobreposição (Overlap Check)
    // Compara cada campo com todos os campos subsequentes para garantir integridade.
    for (i, f1) in parsed_fields.iter().enumerate() {
        for f2 in &parsed_fields[i + 1..] {

            let overlap_start = std::cmp::max(f1.pos_start, f2.pos_start);
            let overlap_end = std::cmp::min(f1.pos_end, f2.pos_end);

            // Se o início da intersecção for menor ou igual ao fim, houve colisão.
            if overlap_start <= overlap_end {
                let err = syn::Error::new_spanned(
                    &f2.ident, // Aponta o erro no editor para o segundo campo
                    format!(
                        "Conflito de Posição detectado!\nCampo A: '{}' ocupa {}..{}\nCampo B: '{}' ocupa {}..{}\nSobreposição nas posições: {}..{}",
                        f1.ident, f1.pos_start, f1.pos_end,
                        f2.ident, f2.pos_start, f2.pos_end,
                        overlap_start, overlap_end
                    )
                );

                return err.to_compile_error().into();
            }
        }
    }

    // --- GERAÇÃO DO CÓDIGO FINAL ---

    // 4. Gera o vetor de FieldSpec (Definição do Layout)
    // Isso cria o `vec![ FieldSpec { ... }, ... ]` que será usado em tempo de execução.
    let field_specs = parsed_fields.iter().map(|f| {
        let name = f.ident.to_string(); // String em compile-time
        let start = f.pos_start;
        let end = f.pos_end;

        let kind = match &f.kind {
            FieldKindMacro::Alpha => quote!(cnab_fixedwidth::FieldKind::Alpha),
            FieldKindMacro::Numeric => quote!(cnab_fixedwidth::FieldKind::Numeric),
            FieldKindMacro::Decimal { scale } => quote!(cnab_fixedwidth::FieldKind::Decimal { scale: #scale }),
        };

        // Note o uso de `#name` direto, resultando em &'static str no código final
        quote! {
            cnab_fixedwidth::FieldSpec {
                name: #name,
                pos: cnab_fixedwidth::FieldPos { start: #start, end: #end },
                kind: #kind,
            }
        }
    });

    // 5. Gera a inicialização da Struct (Mapeamento Value -> Struct Field)
    // Converte os valores genéricos (Value::Numeric) para os tipos concretos (u32, i64, f64).
    let field_inits = parsed_fields.iter().map(|f| {
        let ident = &f.ident;
        let name = ident.to_string();
        let ty = &f.ty;

        match f.kind {
            FieldKindMacro::Alpha => quote! {
                // Extrai string, garante UTF-8 válido e converte para String owned
                #ident: parsed[#name].as_str()
                    .ok_or(cnab_fixedwidth::FixedWidthError::InvalidUtf8)?
                    .to_string()
            },
            FieldKindMacro::Numeric => quote! {
                // Extrai i64 e faz cast para o tipo do campo (ex: u32, i32, usize)
                // Se falhar o tipo no core (ex: Alpha onde devia ser Num), retorna erro InvalidNumeric
                #ident: parsed[#name].as_i64().ok_or(
                    cnab_fixedwidth::FixedWidthError::InvalidNumeric {
                        field: #name,
                        snippet: String::new(),
                    }
                )? as #ty
            },
            FieldKindMacro::Decimal { scale: _ } => quote! {
                // Extrai f64 (já ajustado pela escala no core)
                #ident: parsed[#name].as_f64().ok_or(
                    cnab_fixedwidth::FixedWidthError::InvalidNumeric {
                        field: #name,
                        snippet: String::new(),
                    }
                )?
            },
        }
    });

    // 6. Bloco final de implementação
    quote! {
        impl cnab_fixedwidth::FixedWidthParse for #name {
            fn parse(line: &str) -> cnab_fixedwidth::Result<Self> {
                // Criação da lista de especificações (barato pois são literais estáticos)
                let fields = vec![ #(#field_specs),* ];

                // Chamada ao parser genérico do Core
                let parsed = cnab_fixedwidth::parse_line(line, &fields)?;

                // Construção da Struct segura
                Ok(Self {
                    #(#field_inits),*
                })
            }
        }
    }.into()
}