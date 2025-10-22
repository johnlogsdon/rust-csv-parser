# Rust CSV Parser

A high-performance, memory-efficient CSV parser written in Rust that processes data in chunks for optimal performance with large files.

## Features

- **Streaming Processing**: Parse CSV files larger than available RAM by processing in configurable chunks
- **RFC 4180 Compliant**: Full support for standard CSV format with proper quoting and escaping
- **Configurable Dialects**: Support for different CSV formats (custom delimiters, quote characters, escape sequences)
- **Memory Efficient**: Zero-copy processing where possible, with controlled memory usage
- **Error Recovery**: Comprehensive error handling with detailed error messages
- **Performance Optimized**: State machine-based parsing with minimal allocations

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rust_csv_parser = "0.1.0"
```

Basic usage:

```rust
use rust_csv_parser::{CsvChunkParser, CsvConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create parser with default RFC 4180 settings
    let config = CsvConfig::default();
    let mut parser = CsvChunkParser::new(config);

    // Parse CSV data in chunks
    let csv_data = "name,age,city\nAlice,30,New York\nBob,25,San Francisco";
    let result = parser.process_chunk(csv_data)?;

    // Access parsed rows
    for row in result.complete_rows {
        println!("Row: {:?}", row);
    }

    Ok(())
}
```

## Architecture

This parser uses a **deterministic finite automaton (DFA)** implemented as a state machine for high-performance CSV parsing. Key design principles:

- **Single Responsibility**: Each component (configuration, state machine, builders) has one clear purpose
- **Streaming First**: Process data in chunks to handle arbitrarily large files
- **Memory Conscious**: Minimize allocations through buffer reuse and zero-copy techniques
- **Error Resilient**: Comprehensive error handling with recovery mechanisms

For detailed architectural information, see [DESIGN.md](DESIGN.md).

## API Reference

### CsvConfig

Configure CSV parsing behavior:

```rust
use rust_csv_parser::CsvConfig;

let config = CsvConfig {
    delimiter: ',',      // Field separator
    quote: '"',          // Quote character
    escape: '"',         // Escape character (set to quote for RFC 4180)
};

// Or use defaults (RFC 4180 compliant)
let config = CsvConfig::default();
```

### CsvChunkParser

Main parser interface:

```rust
use rust_csv_parser::CsvChunkParser;

let mut parser = CsvChunkParser::new(config);

// Parse data in chunks
let chunk = "field1,field2\nvalue1,value2";
let result = parser.process_chunk(chunk)?;
```

### ChunkResult

Result of parsing a chunk:

```rust
let result: ChunkResult = parser.process_chunk(chunk)?;

// Complete rows parsed in this chunk
for row in result.complete_rows {
    // Each row is Vec<String>
    println!("Parsed row: {:?}", row);
}

// Leftover data for next chunk (partial rows)
if !result.leftover_data.is_empty() {
    // Save for next processing cycle
}
```

## Advanced Usage

### Custom CSV Dialects

```rust
// Tab-separated values (TSV)
let tsv_config = CsvConfig {
    delimiter: '\t',
    quote: '"',
    escape: '"',
};

// Pipe-separated values
let psv_config = CsvConfig {
    delimiter: '|',
    quote: '"',
    escape: '"',
};
```

### Processing Large Files

```rust
use std::fs::File;
use std::io::Read;

fn process_large_file(filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = CsvChunkParser::new(CsvConfig::default());
    let mut file = File::open(filename)?;
    let mut buffer = [0u8; 8192]; // 8KB chunks

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break; // EOF
        }

        let chunk = std::str::from_utf8(&buffer[..bytes_read])?;
        let result = parser.process_chunk(chunk)?;

        // Process complete rows
        for row in result.complete_rows {
            process_row(row);
        }

        // Handle leftover data if needed
        if !result.leftover_data.is_empty() {
            // This would be prepended to next chunk
        }
    }

    Ok(())
}
```

### Error Handling

```rust
match parser.process_chunk(chunk) {
    Ok(result) => {
        // Process successful result
        for row in result.complete_rows {
            println!("Row: {:?}", row);
        }
    }
    Err(rust_csv_parser::CsvError::UnclosedQuote) => {
        eprintln!("Error: Unclosed quote in CSV data");
    }
    Err(rust_csv_parser::CsvError::DataAfterClosingQuote(ch)) => {
        eprintln!("Error: Unexpected character '{}' after closing quote", ch);
    }
    Err(other) => {
        eprintln!("Parse error: {:?}", other);
    }
}
```

## Performance

The parser achieves high throughput through optimized systems programming techniques:

### Technical Optimizations

- State machine with direct jump table dispatch for predictable execution
- Zero-copy processing where possible to minimize memory allocation
- Buffer reuse between operations to maintain allocation efficiency
- Chunked processing with constant memory usage regardless of input size

### Benchmark Results

On Apple M3 Max, single-threaded (UTF-8 safe implementation):
- 122 MB/s sustained throughput
- 471,000 rows/second processing rate
- 16.5 million fields/second field processing rate
- 3-4 MB heap usage regardless of data size

### Performance Characteristics

- **Single-threaded design**: Predictable performance without thread contention or synchronization overhead
- **Horizontal scaling**: Run multiple parser instances across CPU cores for parallel processing
- Processes large CSV files efficiently with bounded memory growth
- Maintains consistent performance across different data patterns
- Handles quoted fields and special characters without performance degradation
- Suitable for high-throughput data processing applications

Run benchmarks: `cargo bench --bench parser_stability`

### Testing Environment
- **Hardware**: Apple M3 Max
- **Benchmark**: 30-second sustained throughput test
- **Data**: Synthetic CSV with 35 fields per row, quoted fields, and special characters
- **Memory Monitoring**: Real-time heap usage tracking

## Testing

### Unit Tests

Run all unit tests:

```bash
cargo test
```

The test suite covers:
- Basic CSV parsing functionality
- Quoted field handling
- Error conditions
- Chunked processing edge cases
- Custom delimiter support

### Performance Benchmarks

Run performance benchmarks:

```bash
# Time-based benchmark with synthetic data
cargo bench --bench parser_stability

# With custom duration (seconds)
CSV_BENCH_DURATION=30 cargo bench --bench parser_stability
```

#### Test Data Generation

The benchmark uses a sophisticated synthetic data generator that creates realistic CSV patterns:

**Row Template**: Each CSV row contains 35 fields representing a complete e-commerce transaction record:
- Client ID, marketplace, transaction details
- Financial amounts, currency codes, item information
- Shipping addresses with quoted fields containing special characters
- Order and invoice identifiers, dates

**Example Row** (formatted for readability):
```csv
"CLIENT_0,000000,001",SHOPIFY,SHOPIFY,SALE,TXN_1_ROW_1000,25.99,21.99,GBP,"Beauty Power Duo",SKU_000001,GB,GB,10,,Dr Smith,"123 Main's Street\n",,London,SW1A 1AA,ORD_000001,INV_000001,2024-01-01,2024-01-01,2024-01-01,2024-01-01,,,,CON_1,NO,,false,shopify,file_1.json,15.99
```

**Chunk Generation**: Each benchmark iteration creates a ~64KB data chunk containing:
- **165 complete rows** (~400 bytes each) for bulk processing
- **1 partial row** (truncated mid-field) to test chunking edge cases

**Why This Design?**
- Tests realistic CSV complexity with quoted fields, escaped characters, and empty fields
- Ensures chunking logic works correctly with incomplete rows at boundaries
- Provides consistent, reproducible performance measurements
- Simulates real-world e-commerce data patterns

The benchmark runs continuously for the specified duration (default: 10 seconds, configurable via `CSV_BENCH_DURATION`), processing thousands of chunks to measure sustained throughput under realistic conditions.

#### Benchmark Scope and Limitations

**What it measures**: Pure CSV parsing performance with in-memory data generation
- Raw parsing throughput without I/O overhead
- Memory-to-memory processing speed
- Parser efficiency under continuous load

**What it does NOT measure**:
- File I/O performance (disk reads, buffering, memory mapping)
- Network data streaming scenarios
- Memory allocation patterns of real applications
- System-level bottlenecks (disk I/O, memory bandwidth limits)
- Concurrent processing overhead

These results represent the theoretical maximum performance of the parser itself. Real-world applications will typically see 20-50% lower throughput due to I/O and system overhead.

#### File-Based Benchmarking Considerations

**Why not file-based benchmarks?**
Reading from a pre-generated CSV file introduces OS caching effects that make results unrealistic:
- **First read**: Measures actual disk I/O performance (~50-200 MB/s typical)
- **Subsequent reads**: OS caches file in memory, subsequent runs measure RAM-to-RAM throughput (~5000+ MB/s)
- **Inconsistent results**: Performance varies dramatically between runs based on cache state

**Alternative approaches** (not currently implemented):
- **Memory-mapped files**: `mmap()` provides virtual memory interface to files
- **Direct I/O**: Bypass OS cache with `O_DIRECT` flag (platform-specific)
- **Multiple file rotation**: Read different files to avoid cache hits
- **Cache warming runs**: Include cache-warming phase before measurement

The current in-memory approach provides consistent, reproducible measurements of the parser's core performance.

## Error Types

The parser provides detailed error information:

- `UnclosedQuote`: Quoted field not properly closed
- `DataAfterClosingQuote(char)`: Unexpected data after quote
- `UnexpectedEndOfFile`: Premature end of input
- `Utf8Error`: Invalid UTF-8 encoding in input

## Design Philosophy

This crate demonstrates Rust's capabilities for systems programming:

- Memory Safety: No null pointer dereferences, buffer overflows, or data races
- Performance: Zero-cost abstractions with predictable runtime behavior
- Reliability: Comprehensive error handling and recovery
- Maintainability: Clear separation of concerns with well-defined interfaces

## Contributing

See [DESIGN.md](DESIGN.md) for detailed architectural information and development guidelines.

## License

This project is open source. See the license file for details.
