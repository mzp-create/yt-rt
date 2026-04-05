use boa_engine::{Context, Source};

/// A lightweight JavaScript interpreter backed by the Boa engine.
pub struct JsInterpreter {
    context: Context,
}

impl JsInterpreter {
    pub fn new() -> Self {
        Self {
            context: Context::default(),
        }
    }

    /// Execute JavaScript code and return the result as a string.
    pub fn execute(&mut self, code: &str) -> anyhow::Result<String> {
        let result = self
            .context
            .eval(Source::from_bytes(code))
            .map_err(|e| anyhow::anyhow!("JS execution error: {e}"))?;

        let output = result
            .to_string(&mut self.context)
            .map_err(|e| anyhow::anyhow!("JS conversion error: {e}"))?;

        Ok(output.to_std_string_escaped())
    }

    /// Call a named function with string arguments.
    pub fn call_function(&mut self, func_name: &str, args: &[&str]) -> anyhow::Result<String> {
        let args_str = args
            .iter()
            .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(",");
        let code = format!("{}({})", func_name, args_str);
        self.execute(&code)
    }

    /// Load JavaScript code into the interpreter context (for later function calls).
    pub fn load(&mut self, code: &str) -> anyhow::Result<()> {
        self.context
            .eval(Source::from_bytes(code))
            .map_err(|e| anyhow::anyhow!("JS load error: {e}"))?;
        Ok(())
    }
}

impl Default for JsInterpreter {
    fn default() -> Self {
        Self::new()
    }
}
