use super::{normalize, recovery::*};
use linter::Error;
use std::path::Path;
use tree_sitter::{Parser, Tree};

pub(crate) fn parse(path: &Path, source: &str) -> Result<Tree, Error> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|error| parse_error(path, error.to_string()))?;
    let stages: [fn(&str) -> String; 10] = [
        normalize::macro_body_comments,
        normalize::complex_macro,
        normalize::function_pointer_annotations,
        normalize::atomic_specifiers,
        normalize::gnu_attributes,
        normalize::computed_goto,
        normalize::offsetof_designators,
        normalize::va_arg_types,
        normalize::named_registers,
        normalize::directives_inside_parentheses,
    ];
    let normalized = stages.into_iter().fold(
        source.replace("_Thread_local", "             "),
        |text, stage| stage(&text),
    );
    let normalized = normalize::declared_macro_lines(source, &normalized);
    let tree = parser
        .parse(&normalized, None)
        .ok_or_else(|| parse_error(path, "parser returned no syntax tree"))?;
    if let Some(node) = first_unrecoverable_error(tree.root_node(), source) {
        let point = node.start_position();
        let excerpt = node
            .utf8_text(source.as_bytes())
            .unwrap_or("<non-UTF-8 syntax>")
            .lines()
            .next()
            .unwrap_or_default();
        return Err(parse_error(
            path,
            format!(
                "source contains invalid C syntax at {}:{} ({}, {excerpt:?})",
                point.row + 1,
                point.column + 1,
                node.kind()
            ),
        ));
    }
    Ok(tree)
}
fn parse_error(path: &Path, message: impl Into<String>) -> Error {
    Error::Analysis(format!("{}: {}", path.display(), message.into()))
}
#[cfg(test)]
mod test {
    use super::parse;
    use std::path::Path;

    #[test]
    fn condition_assembled_across_a_preprocessor_conditional_parses() {
        let source = "int f(int a, int b) {\n    if (a != 0 ||\n#if !defined(SKIP)\n    \
            \u{20}   b != 0 ||\n#endif\n        a == b)\n        return 1;\n    return 0\
            ;\n}\n";
        assert!(parse(Path::new("conditional.c"), source).is_ok());
    }

    #[test]
    fn an_apostrophe_inside_a_comment_does_not_capture_the_parenthesis_scan() {
        let source = "/* the caller doesn't own (this) */\n#ifndef GUARD_H\n#define GUAR\
            D_H\nint f(void);\n#endif\n";
        assert!(parse(Path::new("guard.h"), source).is_ok());
    }

    #[test]
    fn a_comment_inside_a_continued_definition_does_not_leak_the_body() {
        let source = "#define WARM(address, warm)                                  \\\n\
                      \x20   do {                                                     \\\n\
                      \x20       caught = 0;                                          \\\n\
                      \x20       if (warm) sink += *(const char *)(warm); /* warm */  \\\n\
                      \x20       touch(address);                                      \\\n\
                      \x20   } while (0)\n\n\
                      /* A note between the definition and the declaration that\n\
                      \x20* runs on to a second line. */\n\
                      static unsigned long long load(const volatile void *address) {\n\
                      \x20   return 0;\n\
                      }\n";
        parse(Path::new("coarse.c"), source).unwrap();
    }

    #[test]
    fn a_definition_continued_past_the_last_line_has_no_position_off_the_end() {
        let source = "#define DISPATCH(context) \\\n\
                      \x20   step(context); \\\n\
                      \x20   /* the note runs on \\\n\
                      \x20    * to a second line */ \\\n\
                      \x20   if ((context)->ready) { \\\n\
                      \x20   } \\\n";
        assert!(parse(Path::new("dispatch.h"), source).is_ok());
    }

    #[test]
    fn parser_accepts_valid_c() {
        assert!(parse(Path::new("valid.c"), "int answer(void) { return 42; }").is_ok());
    }

    #[test]
    fn parser_rejects_recovered_syntax_errors() {
        let error = parse(
            Path::new("invalid.c"),
            "int answer(void) { return ; trailing }",
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid C syntax"));
    }

    #[test]
    fn parser_accepts_defined_top_level_macro_with_an_empty_argument() {
        let source = "#define MAKE(name, ty, suffix) ty name(ty value) { return value; }\n\
                      MAKE(identity, int, )\n";
        parse(Path::new("generated.c"), source).unwrap();
    }

    #[test]
    fn parser_rejects_undeclared_top_level_recovery() {
        assert!(parse(Path::new("invalid.c"), "UNKNOWN(identity, int, )\n").is_err());
    }

    #[test]
    fn parser_accepts_declared_function_scope_macro_invocation() {
        let source = "#define EACH_FIELD(X) X(first) X(second)\n\
                      int valid(int first, int second) {\n\
                          EACH_FIELD(VALIDATE)\n\
                          return 1;\n\
                      }\n";
        assert!(parse(Path::new("function-macro.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_undeclared_function_scope_recovery() {
        let source = "int invalid(void) {\n UNKNOWN_MACRO(value)\n return 0;\n}\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_error_node_covering_a_multiline_definition() {
        let source = "#define DISPATCH(context) \\\n+                          if ((cont\
            ext)->ready) { \\\n+                              continue; \\\n+           \
            \u{20}              } else { \\\n+                              break; \\\n+\
            \u{20}                         }\n";
        assert!(parse(Path::new("dispatch.h"), source).is_ok());
    }

    #[test]
    fn parser_accepts_error_on_final_uncontinued_macro_line() {
        let source = "#define BODY(value) \\\n+                          do { \\\n+     \
            \u{20}                        value++; \\\n+                          } whil\
            e (0)\n";
        assert!(parse(Path::new("dispatch.h"), source).is_ok());
    }

    #[test]
    fn parser_accepts_function_after_uncontinued_macro_body() {
        let source = "#define BODY(value) do { \\\n+                          value++; \
            \\\n+                      } while (0)\n\nint main(void) { return 0; }\n";
        assert!(parse(Path::new("macro.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_comment_after_uncontinued_macro_body() {
        let source = "#define BODY(value) do { \\\n+                          value++; \
            \\\n+                      } while (0)\n\n/* next macro */\n#define NEXT 1\n";
        assert!(parse(Path::new("macro.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_missing_semicolon_before_function() {
        let source = "int value(void) { return 1 }\n\nint main(void) { return 0; }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_rejects_missing_closing_brace_after_function_macro() {
        let source = "int main(void) { return 0;\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_multiline_invocation_of_a_declared_macro() {
        let source = "#define SIGNATURE(value, type) _Generic((value), type: 1, default: 0)\n\
                      _Static_assert(SIGNATURE(&function,\n\
                                               void (*)(void)),\n\
                                     \"signature changed\");\n";
        assert!(parse(Path::new("signature.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_consecutive_declared_function_macros() {
        let source = "#define FUNCTION(name) static void name(void) {}\n\
                      FUNCTION(first)\n\
                      FUNCTION(second)\n\
                      int main(void) { first(); second(); return 0; }\n";
        assert!(parse(Path::new("functions.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_declared_macro_in_inline_assembly_operands() {
        let source = "#define CLOBBERS \"memory\", \"cc\"\n\
                      void barrier(void) { __asm__ volatile(\"\" : : : CLOBBERS); }\n";
        assert!(parse(Path::new("assembly.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_undeclared_macro_in_inline_assembly_operands() {
        let source = "void barrier(void) { __asm__ volatile(\"\" : : : CLOBBERS); }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_rejects_multiline_invocation_of_an_undeclared_macro() {
        let source = "_Static_assert(UNKNOWN(&function,\n\
                                             void (*)(void)),\n\
                                   \"signature changed\");\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_c11_thread_local_storage() {
        let source = "typedef struct Options Options;\nstatic _Thread_local Options *current;\n";
        assert!(parse(Path::new("storage.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_standard_complex_type_macro() {
        let source = "double magnitude(double complex value) { return 0; }\n";
        assert!(parse(Path::new("complex.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_gnu_computed_goto() {
        let source = "int run(void **table, int index) { goto *table[index]; target: return 0; }\n";
        assert!(parse(Path::new("goto.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_gnu_named_register_declaration() {
        let source = "void run(void) { register unsigned long value __asm__(\"r15\") = 1; }\n";
        assert!(parse(Path::new("register.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_short_gnu_named_register_spelling() {
        let source = "void run(void) { register unsigned long value asm(\"x0\") = 1; }\n";
        assert!(parse(Path::new("register.c"), source).is_ok());
    }

    #[test]
    fn parser_does_not_consume_inline_assembly_as_a_named_register() {
        let source = "void run(void) { asm(\"instruction %0\" : : \"r\"(1)); }\n";
        assert!(parse(Path::new("assembly.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_unclosed_gnu_named_register_declaration() {
        let source = "void run(void) { register unsigned long value __asm__(\"r15\" = 1; }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_rejects_unterminated_gnu_computed_goto() {
        let source = "int run(void **table, int index) { goto *table[index] }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_parenthesized_c11_atomic_type() {
        let source = "typedef struct Host Host;\nstatic _Atomic(const Host *) current;\n";
        assert!(parse(Path::new("atomic.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_gnu_attribute_after_declarator() {
        let source = "void release(void *);\nvoid *value __attribute__((cleanup(release))) = 0;\n";
        assert!(parse(Path::new("attribute.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_unclosed_gnu_attribute() {
        let source = "void *value __attribute__((cleanup(release)) = 0;\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_rejects_unclosed_parenthesized_c11_atomic_type() {
        let source = "typedef struct Host Host;\nstatic _Atomic(const Host * current;\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_builtin_offsetof_with_a_struct_type() {
        let source = "struct cpu { unsigned long sigmask; };\n\
                      int offset(void) { return (int)__builtin_offsetof(struct cpu, sigmask); }\n";
        assert!(parse(Path::new("offset.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_va_arg_with_pointer_type() {
        let source =
            "#include <stdarg.h>\nvoid *next(va_list args) { return va_arg(args, void **); }\n";
        assert!(parse(Path::new("varargs.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_unclosed_va_arg_with_pointer_type() {
        let source =
            "#include <stdarg.h>\nvoid *next(va_list args) { return va_arg(args, void **; }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_offsetof_nested_member_designator() {
        let source = "struct pair { int high; }; union value { struct pair parts; };\n\
                      int offset(void) { return (int)offsetof(union value, parts.high); }\n";
        assert!(parse(Path::new("offset.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_unclosed_offsetof_nested_member_designator() {
        let source = "int offset(void) { return (int)offsetof(union value, parts.high; }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_uppercase_function_annotation() {
        let source = "PUBLIC_API int answer(void) { return 42; }\n";
        assert!(parse(Path::new("annotated.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_uppercase_calling_convention_annotation() {
        let source = "static void CALLBACK wait_callback(void) {}\n";
        assert!(parse(Path::new("annotated.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_uppercase_function_pointer_calling_convention() {
        let source = "typedef long(NTAPI *clone_fn)(unsigned long, void *);\n";
        assert!(parse(Path::new("annotated.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_arbitrary_tokens_before_a_function() {
        let source = "not_an_annotation int answer(void) { return 42; }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_rejects_lowercase_calling_convention_tokens() {
        let source = "static void not_an_annotation wait_callback(void) {}\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_accepts_conditional_single_statement_after_if() {
        let source = "int open_file(int access) {\n\
                          int flags;\n\
                          if (access)\n\
                      #ifdef FEATURE_FLAG\n\
                              flags = 1;\n\
                      #else\n\
                              flags = 2;\n\
                      #endif\n\
                          return flags;\n\
                      }\n";
        assert!(parse(Path::new("conditional.c"), source).is_ok());
    }

    #[test]
    fn parser_accepts_preprocessor_selected_else_if_arm() {
        let source = "int inspect(int status) {\n\
                          if (status == 1) { return 1; }\n\
                      #ifdef FEATURE_FLAG\n\
                          else if (status == 2) { return 2; }\n\
                      #endif\n\
                          else { return 0; }\n\
                      }\n";
        assert!(parse(Path::new("conditional.c"), source).is_ok());
    }

    #[test]
    fn parser_rejects_unmatched_else_after_directive() {
        let source = "int inspect(int status) {\n\
                      #ifdef FEATURE_FLAG\n\
                          return status;\n\
                      #endif\n\
                          else { return 0; }\n\
                      }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }

    #[test]
    fn parser_rejects_unclosed_conditional_after_if() {
        let source = "int invalid(int access) {\n\
                          if (access)\n\
                      #ifdef FEATURE_FLAG\n\
                              return 1;\n\
                      }\n";
        assert!(parse(Path::new("invalid.c"), source).is_err());
    }
}
