use vergil_solidity::tools::{detect, ToolInfo};

const GREEN_CHECK: &str = "\x1b[32m✓\x1b[0m";
const RED_CROSS: &str = "\x1b[31m✗\x1b[0m";

pub fn run() -> Result<(), u8> {
    let tools = detect();
    print_table(&tools);
    let missing = tools.iter().filter(|t| !t.found()).count();
    if missing == 0 {
        println!("\nAll {} tools detected.", tools.len());
        Ok(())
    } else {
        println!(
            "\n{} of {} tools missing. Run scripts/install-deps.sh to install.",
            missing,
            tools.len()
        );
        Err(1)
    }
}

fn print_table(tools: &[ToolInfo]) {
    let name_width = tools
        .iter()
        .map(|t| t.display_name.len())
        .max()
        .unwrap_or(0);
    println!("{:<name_width$}   STATUS   VERSION", "TOOL");
    println!("{}", "─".repeat(name_width + 30));
    for tool in tools {
        match &tool.version {
            Some(v) => println!(
                "{:<name_width$}   {}        {}",
                tool.display_name, GREEN_CHECK, v
            ),
            None => println!(
                "{:<name_width$}   {}        missing — {}",
                tool.display_name, RED_CROSS, tool.install_hint
            ),
        }
    }
}
