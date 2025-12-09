# CNAB Fixed Width

[![Crates.io](https://img.shields.io/crates/v/cnab-fixed-width.svg)](https://crates.io/crates/cnab-fixedwidth)
[![Documentation](https://docs.rs/cnab-fixed-width/badge.svg)](https://docs.rs/cnab-fixedwidth)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A robust, type-safe, and declarative Rust parser for fixed-width files, specifically designed for Brazilian Banking Standards (**CNAB 240/400**).

### Author: Fabio Covolo Mazzo (fabiomazzo@gmail.com)

## 🚀 Why this crate?

Parsing CNAB files is notoriously error-prone. Most libraries use 0-based indexing (Python/C style), while banking manuals use **1-based inclusive indexing**. Converting between them manually is a source of bugs.

**CNAB Fixed Width** solves this by allowing you to copy definitions straight from the PDF manuals into your Rust structs.

### Features

- **CNAB Friendly:** Uses `start..end` positions exactly as they appear in banking documentation (1-based, inclusive).
- **Compile-Time Safety:** Detects overlapping fields during compilation. If you define a field at `1..10` and another at `10..20`, your code won't compile.
- **Type Safety:** automatically handles `Numeric` (integer), `Decimal` (implied scaling), and `Alpha` (text trimming).
- **High Performance:** Zero-allocation field definition (uses `&'static str` and macro-generated parsers).

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
cnab-fixed-width = "0.1.0"
```

## ⚡ Usage

```rust
use cnab_fixed_width::{FixedWidth, FixedWidthParse};

#[derive(Debug, FixedWidth)]
pub struct HeaderArquivo {
    // Defines a numeric field from position 1 to 3 (inclusive)
    #[fw(pos = "1..3", numeric)]
    pub codigo_banco: u32,

    // Defines a numeric field from 4 to 7
    #[fw(pos = "4..7", numeric)]
    pub lote_servico: u32,

    // Defines a string field. Trims whitespace automatically.
    #[fw(pos = "103..132", alpha)]
    pub nome_empresa: String,

    // Defines a decimal field.
    // "000000001234" with scale 2 becomes 12.34
    #[fw(pos = "133..144", decimal = 2)]
    pub valor_total: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let line = "3410000... (rest of the 240 char line) ...";
    
    let header = HeaderArquivo::parse(line)?;
    
    println!("Banco: {}", header.codigo_banco);
    println!("Empresa: {}", header.nome_empresa);
    println!("Valor: {:.2}", header.valor_total);

    Ok(())
}
```

## 🛠️ Attributes Reference

The #[fw(...)] attribute supports the following options:
### Position (pos)
Required. Defines the start and end positions (inclusive, 1-based).

* Format: "start..end"
* Example: pos = "1..3" captures characters 1, 2, and 3.

### Data Types (Choose one)
| Attribute |	Rust Type |	Description |
|-----------|-------------|------------|
| alpha |	String	|Alphanumeric text. Trims trailing spaces. |
| numeric	| u32, i64, etc. |	Integer numbers. Trims padding spaces/zeros. Returns error if non-digits are found.|
| decimal = N	|f64	Numeric value with implied decimals.| N is the number of decimal places.|


## 🛡️ Error Handling
The parser is strict. It will return an error if:
* The line is shorter than the required fields.
* A numeric field contains letters.
* UTF-8 decoding fails.

## 🚨 Compile-Time Checks
The macro validates your layout. The following code will not compile:
