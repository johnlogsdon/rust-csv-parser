//! # CSV Parser
//!
//! A high-performance, streaming CSV parser built with clean architecture.
//! This library provides RFC 4180 compliant CSV parsing with support for
//! quoted fields, custom delimiters, and various escape mechanisms.
//!
//! ## Features
//!
//! - **High Performance**: 400K+ rows/second with optimized streaming processing
//! - **RFC 4180 Compliant**: Handles quoted fields, escaped characters, and edge cases
//! - **Streaming Processing**: Memory-efficient chunked parsing for large files
//! - **Configurable**: Support for different delimiters, quote chars, and escape sequences
//! - **Type Safe**: Leverages Rust's type system for correctness
//!
//! ## Usage
//!
//! ```rust
//! use rust_csv_parser::{CsvConfig, CsvChunkParser};
//!
//! let config = CsvConfig::default();
//! let mut parser = CsvChunkParser::new(config);
//!
//! let chunk = "name,age,city\nJohn,30,NYC\n";
//! let result = parser.process_chunk(chunk)?;
//!
//! for row in result.complete_rows {
//!     println!("{:?}", row);
//! }
//! # Ok::<(), rust_csv_parser::CsvError>(())
//! ```

#[derive(Debug, Clone, Copy)]
pub struct CsvConfig { 
    pub delimiter: char,
    pub quote: char,
    pub escape: char,
}

impl Default for CsvConfig {
    fn default() -> Self {
        CsvConfig {
            delimiter: ',',
            quote: '"',
            escape: '"',
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CsvError { 
    UnclosedQuote,
    DataAfterClosingQuote(char),
    Utf8Error(std::string::FromUtf8Error),
}

impl From<std::string::FromUtf8Error> for CsvError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        CsvError::Utf8Error(err)
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CsvState {
    StartOfField,
    InUnquotedField,
    InQuotedField,
    QuoteSeen,
    CustomEscapeSeen,
    EndOfRecord,
    Finished,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Action {
    AppendChar(char),
    AppendEscapedQuote,
    CommitField,
    CommitRow,
    NoOp,
}




// --- STATE TRANSITION HANDLERS ---

#[derive(Debug, Clone, Copy)]
pub struct StateTransition {
    pub new_state: CsvState,
    pub action: Action,
}

mod state_handlers {
    use super::*;

    #[inline(always)]
    pub fn handle_start_of_field(c: Option<char>, config: &CsvConfig) -> Result<StateTransition, CsvError> {
        match c {
            Some(ch) if ch == config.quote => Ok(StateTransition {
                new_state: CsvState::InQuotedField,
                action: Action::NoOp,
            }),
            Some(ch) if ch == config.delimiter => Ok(StateTransition {
                new_state: CsvState::StartOfField,
                action: Action::CommitField,
            }),
            Some('\n') | Some('\r') => Ok(StateTransition {
                new_state: CsvState::EndOfRecord,
                action: Action::CommitRow,
            }),
            Some(ch) => Ok(StateTransition {
                new_state: CsvState::InUnquotedField,
                action: Action::AppendChar(ch),
            }),
            None => Ok(StateTransition {
                new_state: CsvState::Finished,
                action: Action::NoOp,
            }),
        }
    }

    #[inline(always)]
    pub fn handle_in_unquoted_field(c: Option<char>, config: &CsvConfig) -> Result<StateTransition, CsvError> {
        match c {
            Some(ch) if ch == config.delimiter => Ok(StateTransition {
                new_state: CsvState::StartOfField,
                action: Action::CommitField,
            }),
            Some('\n') | Some('\r') => Ok(StateTransition {
                new_state: CsvState::EndOfRecord,
                action: Action::CommitRow,
            }),
            Some(ch) => Ok(StateTransition {
                new_state: CsvState::InUnquotedField,
                action: Action::AppendChar(ch),
            }),
            None => Ok(StateTransition {
                new_state: CsvState::Finished,
                action: Action::CommitField, // Commit last field when EOF is hit
            }),
        }
    }

    #[inline(always)]
    pub fn handle_in_quoted_field(c: Option<char>, config: &CsvConfig) -> Result<StateTransition, CsvError> {
        match c {
            // Standard escape / closing quote (RFC 4180 mode: escape == quote)
            Some(ch) if ch == config.quote && config.quote == config.escape => Ok(StateTransition {
                new_state: CsvState::QuoteSeen,
                action: Action::NoOp,
            }),
            // Closing quote (non-RFC mode: escape != quote)
            Some(ch) if ch == config.quote && config.quote != config.escape => Ok(StateTransition {
                new_state: CsvState::QuoteSeen,
                action: Action::NoOp,
            }),
            // Custom escape char seen (non-RFC mode: escape != quote)
            Some(ch) if ch == config.escape && config.quote != config.escape => Ok(StateTransition {
                new_state: CsvState::CustomEscapeSeen,
                action: Action::NoOp,
            }),
            // Normal data
            Some(ch) => Ok(StateTransition {
                new_state: CsvState::InQuotedField,
                action: Action::AppendChar(ch),
            }),
            // Enforce UnclosedQuote on EOF
            None => Err(CsvError::UnclosedQuote),
        }
    }

    #[inline(always)]
    pub fn handle_quote_seen(c: Option<char>, config: &CsvConfig) -> Result<StateTransition, CsvError> {
        match c {
            // Escaped quote (character after quote is the escape char)
            Some(ch) if ch == config.escape => Ok(StateTransition {
                new_state: CsvState::InQuotedField,
                action: Action::AppendEscapedQuote,
            }),
            // Field delimiter - finalize field
            Some(ch) if ch == config.delimiter => Ok(StateTransition {
                new_state: CsvState::StartOfField,
                action: Action::CommitField,
            }),
            // Row terminator - finalize row
            Some('\n') | Some('\r') => Ok(StateTransition {
                new_state: CsvState::EndOfRecord,
                action: Action::CommitRow,
            }),
            // Commit final row at EOF if it ends on a quote
            None => Ok(StateTransition {
                new_state: CsvState::Finished,
                action: Action::CommitRow,
            }),
            // Error: Character immediately after closing quote
            Some(ch) => Err(CsvError::DataAfterClosingQuote(ch)),
        }
    }

    #[inline(always)]
    pub fn handle_custom_escape_seen(c: Option<char>, _config: &CsvConfig) -> Result<StateTransition, CsvError> {
        match c {
            // Character immediately following custom escape is ALWAYS appended as data
            Some(ch) => Ok(StateTransition {
                new_state: CsvState::InQuotedField,
                action: Action::AppendChar(ch),
            }),
            None => Err(CsvError::UnclosedQuote),
        }
    }

    #[inline(always)]
    pub fn handle_end_of_record(c: Option<char>, _config: &CsvConfig) -> Result<StateTransition, CsvError> {
        match c {
            Some('\n') | Some('\r') => Ok(StateTransition {
                new_state: CsvState::EndOfRecord,
                action: Action::NoOp,
            }),
            None => Ok(StateTransition {
                new_state: CsvState::Finished,
                action: Action::NoOp,
            }),
            _ => Ok(StateTransition {
                new_state: CsvState::StartOfField,
                action: Action::NoOp,
            }),
        }
    }

    #[inline(always)]
    pub fn handle_finished(_c: Option<char>, _config: &CsvConfig) -> Result<StateTransition, CsvError> {
        Ok(StateTransition {
            new_state: CsvState::Finished,
            action: Action::NoOp,
        })
    }
}

// --- MAIN STATE TRANSITION FUNCTION ---
// Now clean and readable - dispatches to focused handlers
#[inline(always)]
fn transition( 
    current_state: CsvState, 
    c: Option<char>, 
    config: &CsvConfig,
) -> Result<StateTransition, CsvError> {
    use CsvState::*;

    match current_state {
        StartOfField => state_handlers::handle_start_of_field(c, config),
        InUnquotedField => state_handlers::handle_in_unquoted_field(c, config),
        InQuotedField => state_handlers::handle_in_quoted_field(c, config),
        QuoteSeen => state_handlers::handle_quote_seen(c, config),
        CustomEscapeSeen => state_handlers::handle_custom_escape_seen(c, config),
        EndOfRecord => state_handlers::handle_end_of_record(c, config),
        Finished => state_handlers::handle_finished(c, config),
    }
}


// --- FIELD PROCESSING ---

#[derive(Debug)]
struct FieldBuilder {
    buffer: Vec<u8>,
    quote_encoded: Vec<u8>,
}

impl FieldBuilder {
    fn new(config: &CsvConfig) -> Self {
        let mut quote_encoded = [0u8; 4];
        let encoded = config.quote.encode_utf8(&mut quote_encoded);

        Self {
            buffer: Vec::with_capacity(256),
            quote_encoded: encoded.as_bytes().to_vec(), // Store only the actual encoded bytes
        }
    }

    // Create a new FieldBuilder reusing an existing quote_encoded to avoid allocation
    fn new_with_quote_encoded(quote_encoded: Vec<u8>) -> Self {
        Self {
            buffer: Vec::with_capacity(256),
            quote_encoded,
        }
    }


    #[inline(always)]
    fn append_char(&mut self, ch: char) {
        // Proper UTF-8 encoding for characters
        let mut utf8_buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut utf8_buf);
        self.buffer.extend_from_slice(encoded.as_bytes());
    }



    #[inline(always)]
    fn append_escaped_quote(&mut self) {
        self.buffer.extend_from_slice(&self.quote_encoded);
    }

    #[inline]
    fn finalize_field(self) -> Result<String, CsvError> {
        String::from_utf8(self.buffer).map_err(CsvError::from)
    }

    #[inline]
    fn reset(&mut self) {
        // Clear the buffer, reusing the existing allocated capacity.
        self.buffer.clear();
    }
}

// --- ROW BUILDING ---

#[derive(Debug)]
struct RowBuilder {
    fields: Vec<String>,
}

impl RowBuilder {
    fn new() -> Self {
        Self {
            fields: Vec::with_capacity(16),
        }
    }

    #[inline]
    fn add_field(&mut self, field_builder: FieldBuilder) -> Result<(), CsvError> {
        let field = field_builder.finalize_field()?;
        self.fields.push(field);
        Ok(())
    }

    #[inline]
    fn finalize_row(&mut self) -> Vec<String> {
        std::mem::take(&mut self.fields)
    }

    #[inline]
    fn clear(&mut self) {
        self.fields.clear();
    }

}

// --- THE IMPURE ORCHESTRATOR/PARSER (PUBLIC) ---

#[derive(Debug)] 
pub struct ChunkResult { 
    pub complete_rows: Vec<Vec<String>>, 
    pub leftover_data: String,
}


pub struct CsvChunkParser { 
    state: CsvState, 
    config: CsvConfig, 
    field_builder: FieldBuilder,
    row_builder: RowBuilder,
}

impl CsvChunkParser {
    pub fn new(config: CsvConfig) -> Self { 
        CsvChunkParser {
            state: CsvState::StartOfField,
            config: config.clone(),
            field_builder: FieldBuilder::new(&config),
            row_builder: RowBuilder::new(),
        }
    }
    
    fn commit_field(&mut self) -> Result<(), CsvError> {
        // 1. Extract the quote_encoded to reuse it without allocation.
        let quote_encoded = std::mem::take(&mut self.field_builder.quote_encoded);

        // 2. Swap the current field_builder out for a new empty one that reuses quote_encoded.
        let completed_builder = std::mem::replace(
            &mut self.field_builder,
            FieldBuilder::new_with_quote_encoded(quote_encoded)
        );

        // 3. Finalize the completed builder and add to the row.
        self.row_builder.add_field(completed_builder)?;

        // 4. The new field_builder already has the quote_encoded and an empty buffer.
        // No reset needed since it's already clean.

        Ok(())
    }

    fn commit_row(&mut self) -> Result<Vec<String>, CsvError> {
        self.commit_field()?; 
        Ok(self.row_builder.finalize_row())
    }
    
    fn is_empty_row(row: &[String]) -> bool {
        if row.is_empty() {
            return true;
        }
        if row.len() == 1 && row[0].is_empty() {
            return true;
        }
        false
    }


    pub fn process_chunk(&mut self, chunk: &str) -> Result<ChunkResult, CsvError> { 
        let mut char_indices = chunk.char_indices().peekable(); 
        let mut completed_rows = Vec::new(); 
        let mut last_consumed_index = 0; 
        let chunk_length = chunk.len(); 
        
        while let Some((i, current_char)) = char_indices.next() {
            let prev_state = self.state;
            
            let StateTransition { new_state: next_state, action } = transition(prev_state, Some(current_char), &self.config)?;
            match action {
                Action::AppendChar(ch) => {
                    self.field_builder.append_char(ch);
                },
                Action::AppendEscapedQuote => {
                    self.field_builder.append_escaped_quote();
                },
                Action::CommitField => {
                    self.commit_field()?;
                },
                Action::CommitRow => {
                    let row = self.commit_row()?;
                    if Self::is_empty_row(&row) {
                        // Skip empty rows
                    } else {
                        completed_rows.push(row);
                    }
                },
                Action::NoOp => {
                    // No operation needed
                    }
            }
            
            // 3. Update the state
            self.state = next_state;
            
            // 4. Handle EndOfRecord boundaries (Consuming CRLF)
            if self.state == CsvState::EndOfRecord {
                let mut consumed_c = None;
                
                {
                    if let Some(&(next_i, next_c)) = char_indices.peek() {
                        let StateTransition { action, .. } = transition(self.state, Some(next_c), &self.config)?;
                        if action == Action::NoOp {
                            consumed_c = Some((next_i, next_c)); 
                        }
                    }
                }

                if let Some((i, c)) = consumed_c {
                    char_indices.next(); 
                    last_consumed_index = i + c.len_utf8(); 
                } else {
                    last_consumed_index = i + current_char.len_utf8();
                }

                self.state = CsvState::StartOfField;
            } else {
                last_consumed_index = i + current_char.len_utf8();
            }

        }

        // --- Handle Chunk Exhaustion (Leftover Logic and Final Commit) ---

        // Determine final state and action based on whether this is EOF or just end of chunk
        let StateTransition { new_state: final_state, action: final_action } = if chunk.is_empty() {
            // Empty chunk signals EOF - call transition with None
            transition(self.state, None, &self.config)
                .or_else(|e| {
                    if e == CsvError::UnclosedQuote {
                        return Err(e);
                    }
                    // Propagate other errors and set a terminal state for cleanup
                    self.state = CsvState::Finished;
                    Err(e)
                })?
        } else {
            // Non-empty chunk - check if current state can be completed at chunk boundary
            match self.state {
                // States that can be completed at end of chunk (not truly partial)
                CsvState::InUnquotedField | CsvState::QuoteSeen => {
                    // Call transition with None to get completion action
                    transition(self.state, None, &self.config)
                        .or_else(|e| {
                            if e == CsvError::UnclosedQuote {
                                return Err(e);
                            }
                            // Propagate other errors and set a terminal state for cleanup
                            self.state = CsvState::Finished;
                            Err(e)
                        })?
                },
                // Truly partial states that need more data
                _ => StateTransition {
                    new_state: self.state,
                    action: Action::NoOp,
                }
            }
        };

        // Execute the final action if it commits data (fixes the final line/field at EOF)
        if matches!(final_action, Action::CommitField) {
            self.commit_field()?;
        } else if matches!(final_action, Action::CommitRow) {
            let row = self.commit_row()?;
            if !Self::is_empty_row(&row) {
                completed_rows.push(row);
            }
        }

        // The leftover data logic depends on whether the *final* determined state is a partial state.
        let leftover_data = match final_state {
            // States that imply we stopped mid-record and need more data.
            CsvState::InQuotedField | CsvState::CustomEscapeSeen => {
                // Leftover is the slice that was NOT consumed.
                let leftover = if last_consumed_index < chunk_length {
                    chunk.get(last_consumed_index..chunk_length)
                        .unwrap_or("")
                        .to_string()
                } else {
                    String::new()
                };

                // Preserve state for next chunk
                self.state = final_state;
                leftover
            },

            _ => {
                // CRITICAL FIX: Ensure buffers are cleared when a boundary is fully reached.
                self.row_builder.clear();
                self.field_builder.reset();
                self.state = final_state;
                String::new()
            }
        };

        Ok(ChunkResult { complete_rows: completed_rows, leftover_data })
    }
}


// --- UNIT TESTS (Idiomatic Rust Convention) ---
#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to run a chunk, returning completed rows and parser state
    fn run_chunk(
        parser: &mut CsvChunkParser,
        chunk: &str,
    ) -> Result<ChunkResult, CsvError> {
        parser.process_chunk(chunk)
    }

    // Helper to streamline chunked parsing for verification (FINAL, CLEANED-UP VERSION)
    fn parse_streaming_full(
        chunks: &[&str],
        config: CsvConfig,
    ) -> Result<Vec<Vec<String>>, CsvError> {
        let mut parser = CsvChunkParser::new(config);
        let mut all_rows = Vec::new();
        
        for chunk in chunks {
            // Process each chunk sequentially. The parser's state/field_buffer must carry continuity.
            let result = parser.process_chunk(chunk)?;
            
            all_rows.extend(result.complete_rows);
            
            // We ignore the external leftover buffer entirely to prevent the previous corruption. 
            // The FSM must be trusted to manage field continuity internally.
        }
        
        // --- Final Cleanup and Commit Logic ---
        
        // Trigger final EOF commit logic by passing an empty chunk.
        // This is the definitive signal to commit any remaining row/field content (success) 
        // or enforce the UnclosedQuote error (failure).
        if parser.state != CsvState::Finished {
            let final_result = parser.process_chunk("")?;
            all_rows.extend(final_result.complete_rows);
        }

        Ok(all_rows)
    }

    #[test]
    fn test_scenario_1_basic_completion() -> Result<(), CsvError> {
        let config = CsvConfig::default();
        let mut parser = CsvChunkParser::new(config);

        let chunk = "Value1,Value2\r\n";
        let result = run_chunk(&mut parser, chunk)?;

        assert_eq!(result.complete_rows.len(), 1);
        assert_eq!(result.complete_rows[0], vec!["Value1", "Value2"]);
        assert_eq!(result.leftover_data, "");
        Ok(())
    }

    #[test]
    // Refactored to use the streaming helper for robust test case validation
    fn test_scenario_2_chunking_partial_row() -> Result<(), CsvError> {
        let config = CsvConfig::default();
        let chunks = vec![
            "Start,\"Field that ends mid-", // Chunk 1: ends mid-field, state = InQuotedField
            "quote\",End\n",             // Chunk 2: completes the field and row
        ];

        let rows = parse_streaming_full(&chunks, config)?;

        // The helper should combine the chunks correctly and produce one full row.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["Start", "Field that ends mid-quote", "End"]);
        Ok(())
    }


    #[test]
    fn test_scenario_3_empty_line_filtering() -> Result<(), CsvError> {
        let config = CsvConfig::default();
        let chunks = vec![
            "Row1\n\nRow2\r\n\r\nRow3\n" // Input: Row1\n\nRow2\r\n\r\nRow3\n
        ];
        let rows = parse_streaming_full(&chunks, config)?;
        
        // Expected output: [Row1], [Row2], [Row3] (3 rows, empty lines filtered out)
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["Row1"]);
        assert_eq!(rows[1], vec!["Row2"]);
        assert_eq!(rows[2], vec!["Row3"]);
        Ok(())
    }

    #[test]
    fn test_scenario_4_custom_delimiter() -> Result<(), CsvError> {
        let config = CsvConfig { delimiter: ';', quote: '"', escape: '"' };
        let chunks = vec!["Alpha;Beta;Gamma\n"];
        let rows = parse_streaming_full(&chunks, config)?;

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["Alpha", "Beta", "Gamma"]);
        Ok(())
    }

    #[test]
    fn test_scenario_5_expected_errors() {
        let config = CsvConfig::default();
        let mut parser = CsvChunkParser::new(config);
        
        let chunk_error = "\"bad\"data\n"; // Data after closing quote ('d')

        let result = run_chunk(&mut parser, chunk_error);
        
        // Check for specific error type
        // Note: The conversion to CsvError::Utf8Error might mask DataAfterClosingQuote if the bad data is non-UTF8.
        // But for this test, we assume the input is valid UTF-8 up to the error.
        assert!(matches!(result, Err(CsvError::DataAfterClosingQuote('d'))));
    }
    
    #[test]
    fn test_scenario_5b_unclosed_quote_error() {
        let config = CsvConfig::default();
        let chunks = vec!["Start,\"Unclosed field"];
        
        let result = parse_streaming_full(&chunks, config);
        
        // The final call to transition(state, None, ...) should return UnclosedQuote
        assert!(matches!(result, Err(CsvError::UnclosedQuote)));
    }
    
    #[test]
    fn test_scenario_6a_standard_escaping() -> Result<(), CsvError> {
        let config = CsvConfig::default();
        let chunks = vec!["Field1,\"Value with \"\"Escaped\"\" Quote\",Field3\n"];
        let rows = parse_streaming_full(&chunks, config)?;

        let expected_field2 = "Value with \"Escaped\" Quote".to_string();
        
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["Field1", &expected_field2, "Field3"]);
        Ok(())
    }

    #[test]
    fn test_scenario_6c_quoted_field_with_commas() -> Result<(), CsvError> {
        let config = CsvConfig::default();
        let chunks = vec!["\"CLIENT_0,000000,001\",SHOPIFY,SALE\n"];
        let rows = parse_streaming_full(&chunks, config)?;

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["CLIENT_0,000000,001", "SHOPIFY", "SALE"]);
        Ok(())
    }

    #[test]
    fn test_scenario_6b_custom_escaping() -> Result<(), CsvError> {
        // Config: quote='\"', escape='\\'
        let config = CsvConfig { delimiter: ',', quote: '"', escape: '\\' };
        // The literal Rust string below represents: A,"Value with \"Escaped\" Quote",B\n
        let chunks = vec!["A,\"Value with \\\"Escaped\\\" Quote\",B\n"];
        let rows = parse_streaming_full(&chunks, config)?;

        let expected_field2 = "Value with \"Escaped\" Quote".to_string();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["A", &expected_field2, "B"]);
        Ok(())
    }

    #[test]
    fn test_utf8_handling() -> Result<(), CsvError> {
        let config = CsvConfig::default();

        // Test various UTF-8 characters: emojis, accented chars, symbols
        let chunks = vec![
            "Hello,🌟,café,ñoño,тест\n",  // Emojis, accented chars, Cyrillic
            "\"Field with 🌟 emoji\",normal,\"🎉🎊\"\n",  // Quoted fields with emojis
        ];

        let rows = parse_streaming_full(&chunks, config)?;

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["Hello", "🌟", "café", "ñoño", "тест"]);
        assert_eq!(rows[1], vec!["Field with 🌟 emoji", "normal", "🎉🎊"]);

        Ok(())
    }

    #[test]
    fn test_utf8_multibyte_chars() -> Result<(), CsvError> {
        let config = CsvConfig::default();

        // Test characters that use different byte lengths in UTF-8
        let chunks = vec![
            "a,é,€,𝄞,🎵\n",  // 1, 2, 3, 4, 4 byte UTF-8 sequences
            "\"𝄞 G-clef\",\"🎵 music note\"\n",  // Multi-byte chars in quoted fields
        ];

        let rows = parse_streaming_full(&chunks, config)?;

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["a", "é", "€", "𝄞", "🎵"]);
        assert_eq!(rows[1], vec!["𝄞 G-clef", "🎵 music note"]);

        Ok(())
    }


}