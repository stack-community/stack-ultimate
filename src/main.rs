use base64;
use clap::{App, Arg};
use clipboard::{ClipboardContext, ClipboardProvider};
use nalgebra;
use native_dialog::MessageDialog;
use native_dialog::MessageType;
use opencv::imgproc::resize;
use opencv::{
    core::{self, Mat},
    highgui, imgcodecs, imgproc,
};
use percent_encoding::percent_decode_str;
use petgraph::dot::{Config, Dot};
use petgraph::Graph;
use rand::seq::SliceRandom;
use regex::Regex;
use rodio::{OutputStream, Sink, Source};
use rusqlite::{Connection, Result};
use rusty_audio::Audio;
use serde_json::{self, json, Value};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader, Error, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread::{self, sleep};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sys_info::{cpu_num, cpu_speed, hostname, mem_info, os_release, os_type};
use tera::{Context, Tera};
use web_view::*;

fn main() {
    let app = App::new("Stack Ultimate")
        .version("1.0.0")
        .author("Stack Programming Community")
        .about("The powerful script language designed with a stack oriented approach for efficient execution. ")
        .arg(Arg::new("script")
            .index(1)
            .value_name("FILE")
            .help("Sets the script file to execution")
            .takes_value(true))
        .arg(Arg::new("debug")
            .short('d')
            .long("debug")
            .help("Enables debug mode"));
    let matches = app.clone().get_matches();

    if let Some(script) = matches.value_of("script") {
        if matches.is_present("debug") {
            let mut stack = Executor::new(Mode::Debug);
            stack.evaluate_program(match get_file_contents(Path::new(&script.to_string())) {
                Ok(code) => code,
                Err(err) => {
                    println!("Error! {err}");
                    return;
                }
            })
        } else {
            let mut stack = Executor::new(Mode::Script);
            stack.evaluate_program(match get_file_contents(Path::new(&script.to_string())) {
                Ok(code) => code,
                Err(err) => {
                    println!("Error! {err}");
                    return;
                }
            })
        }
    } else {
        // Show a title
        println!("Stack Programming Language: Ultimate Edition");
        let mut executor = Executor::new(Mode::Debug);
        // REPL Execution
        loop {
            let mut code = String::new();
            loop {
                let enter = input("> ");
                code += &format!("{enter}\n");
                if enter.is_empty() {
                    break;
                }
            }

            executor.evaluate_program(code)
        }
    }
}

/// Read string of the file
fn get_file_contents(name: &Path) -> Result<String, Error> {
    let mut f = File::open(name)?;
    let mut contents = String::new();
    f.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Get standard input
fn input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut result = String::new();
    io::stdin().read_line(&mut result).ok();
    result.trim().to_string()
}

/// Execution Mode
#[derive(Clone, Debug)]
enum Mode {
    Script, // Script execution
    Debug,  // Debug execution
}

/// Data type
#[derive(Clone, Debug)]
enum Type {
    Number(f64),
    String(String),
    Bool(bool),
    List(Vec<Type>),
    Matrix(Vec<f64>, (usize, usize)),
    Object(String, HashMap<String, Type>),
    Json(Value),
    Binary(Vec<u8>),
    Image(Mat),
    Error(String),
}

/// Implement methods
impl Type {
    /// Show data to display
    fn display(&self) -> String {
        match self {
            Type::Number(num) => num.to_string(),
            Type::String(s) => format!("({})", s),
            Type::Bool(b) => b.to_string(),
            Type::List(list) => {
                let result: Vec<String> = list.iter().map(|token| token.display()).collect();
                format!("[{}]", result.join(" "))
            }
            Type::Error(err) => format!("error:{err}"),
            Type::Json(j) => serde_json::to_string_pretty(&j).unwrap_or("{}".to_string()),
            Type::Object(name, _) => {
                format!("Object<{name}>")
            }
            Type::Binary(i) => format!("Binary<{}>", i.len()),
            Type::Matrix(mx, (_, length)) => {
                let mut matrix = Vec::new();
                let mut buffer = Vec::new();

                let mut count = 0;
                for i in mx {
                    if count < *length {
                        buffer.push(*i);
                        count += 1;
                    } else {
                        matrix.push(buffer.clone());
                        buffer.clear();
                        count = 1;
                        buffer.push(*i);
                    }
                }
                matrix.push(buffer.clone());

                let mut text = "{".to_string();

                for i in matrix.iter() {
                    for j in i.iter() {
                        text += &format!(" {j},")
                    }
                    text.remove(text.len() - 1);
                    text += ";"
                }

                text.remove(text.len() - 1);
                text += "}";
                text
            }
            Type::Image(_) => "{Image}".to_string(),
        }
    }

    /// Get string form data
    fn get_string(&self) -> String {
        match self {
            Type::String(s) => s.to_string(),
            Type::Number(i) => i.to_string(),
            Type::Bool(b) => b.to_string(),
            Type::List(l) => Type::List(l.to_owned()).display(),
            Type::Error(err) => format!("error:{err}"),
            Type::Object(name, _) => {
                format!("Object<{name}>")
            }
            Type::Json(j) => j.as_str().unwrap_or("").to_string(),
            Type::Matrix(mx, (_, length)) => {
                let mut matrix = Vec::new();
                let mut buffer = Vec::new();

                let mut count = 0;
                for i in mx {
                    if count < *length {
                        buffer.push(*i);
                        count += 1;
                    } else {
                        matrix.push(buffer.clone());
                        buffer.clear();
                        count = 1;
                        buffer.push(*i);
                    }
                }
                matrix.push(buffer.clone());

                let mut text = "{".to_string();

                for i in matrix.iter() {
                    for j in i.iter() {
                        text += &format!(" {j},")
                    }
                    text.remove(text.len() - 1);
                    text += ";"
                }

                text.remove(text.len() - 1);
                text += "}";
                text
            }
            Type::Binary(i) => format!("Binary<{}>", i.len()),
            Type::Image(_) => "{Image}".to_string(),
        }
    }

    /// Get number from data
    fn get_number(&self) -> f64 {
        match self {
            Type::String(s) => s.parse().unwrap_or(0.0),
            Type::Number(i) => *i,
            Type::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Type::Json(j) => j.as_f64().unwrap_or(0f64),
            Type::List(l) => l.len() as f64,
            Type::Error(e) => e.parse().unwrap_or(0f64),
            Type::Object(_, object) => object.len() as f64,
            Type::Binary(i) => i.len() as f64,
            _ => 0f64,
        }
    }

    /// Get bool from data
    fn get_bool(&self) -> bool {
        match self {
            Type::String(s) => !s.is_empty(),
            Type::Number(i) => *i != 0.0,
            Type::Bool(b) => *b,
            Type::List(l) => !l.is_empty(),
            Type::Json(j) => j.as_bool().unwrap_or(false),
            Type::Error(e) => e.parse().unwrap_or(false),
            Type::Object(_, object) => object.is_empty(),
            Type::Binary(i) => !i.is_empty(),
            _ => false,
        }
    }

    /// Get list form data
    fn get_list(&self) -> Vec<Type> {
        match self {
            Type::String(s) => s
                .to_string()
                .chars()
                .map(|x| Type::String(x.to_string()))
                .collect::<Vec<Type>>(),
            Type::Number(i) => vec![Type::Number(*i)],
            Type::Bool(b) => vec![Type::Bool(*b)],
            Type::List(l) => l.to_vec(),
            Type::Error(e) => vec![Type::Error(e.to_string())],
            Type::Object(_, object) => object.values().map(|x| x.to_owned()).collect::<Vec<Type>>(),
            Type::Json(j) => {
                if let Some(obj) = j.as_object() {
                    obj.keys().cloned().map(Type::String).collect::<Vec<Type>>()
                } else {
                    Vec::new()
                }
            }
            Type::Binary(i) => i.iter().map(|x| Type::Number(*x as f64)).collect(),
            _ => vec![],
        }
    }

    fn get_matrix(&self) -> (Vec<f64>, (usize, usize)) {
        match self {
            Type::Matrix(mx, size) => (mx.to_vec(), *size),
            _ => (vec![], (0, 0)),
        }
    }

    fn get_json(&self) -> Value {
        match self {
            Type::Json(j) => j.to_owned(),
            Type::String(j) => serde_json::from_str(j).unwrap_or(json!({})),
            _ => json!({}),
        }
    }

    fn get_object(&self) -> (String, HashMap<String, Type>) {
        match self {
            Type::Object(name, value) => (name.to_owned(), value.to_owned()),
            _ => ("".to_string(), HashMap::new()),
        }
    }

    fn get_image(&self) -> Mat {
        match self {
            Type::Image(i) => i.clone(),
            _ => Mat::default(),
        }
    }
}

/// Manage program execution
#[derive(Clone, Debug)]
struct Executor {
    stack: Vec<Type>,              // Data stack
    memory: HashMap<String, Type>, // Variable's memory
    mode: Mode,                    // Execution mode
}

impl Executor {
    /// Constructor
    fn new(mode: Mode) -> Executor {
        Executor {
            stack: Vec::new(),
            memory: HashMap::new(),
            mode,
        }
    }

    /// Output log
    fn log_print(&mut self, msg: String) {
        if let Mode::Debug = self.mode {
            print!("{msg}");
        }
    }

    /// Show variable inside memory
    fn show_variables(&mut self) {
        self.log_print("Variables {\n".to_string());
        let max = self.memory.keys().map(|s| s.len()).max().unwrap_or(0);
        for (name, value) in self.memory.clone() {
            self.log_print(format!(
                " {:>width$}: {}\n",
                name,
                value.display(),
                width = max
            ))
        }
        self.log_print("}\n".to_string())
    }

    /// Show inside the stack
    fn show_stack(&mut self) -> String {
        format!(
            "Stack〔 {} 〕",
            self.stack
                .iter()
                .map(|x| x.display())
                .collect::<Vec<_>>()
                .join(" | ")
        )
    }

    /// Parse token by analyzing syntax
    /// Parse token by analyzing syntax
    fn analyze_syntax(&mut self, code: String) -> Vec<String> {
        // Convert tabs, line breaks, and full-width spaces to half-width spaces
        let code = code.replace(['\n', '\t', '\r', '　'], " ");

        let mut syntax = Vec::new(); // Token string
        let mut buffer = String::new(); // Temporary storage
        let mut brackets = 0; // String's nest structure
        let mut parentheses = 0; // List's nest structure
        let mut braces = 0; // Matrix's nest structure
        let mut hash = false; // Is it Comment
        let mut escape = false; // Flag to indicate next character is escaped

        for c in code.chars() {
            match c {
                '\\' if !escape => {
                    escape = true;
                }
                '(' if !hash && !escape => {
                    brackets += 1;
                    buffer.push('(');
                }
                ')' if !hash && !escape => {
                    brackets -= 1;
                    buffer.push(')');
                }
                '{' if !hash && brackets == 0 && !escape => {
                    braces += 1;
                    buffer.push('{');
                }
                '}' if !hash && brackets == 0 && !escape => {
                    braces -= 1;
                    buffer.push('}');
                }
                '#' if !hash && !escape => {
                    hash = true;
                    buffer.push('#');
                }
                '#' if hash && !escape => {
                    hash = false;
                    buffer.push('#');
                }
                '[' if !hash && brackets == 0 && !escape => {
                    parentheses += 1;
                    buffer.push('[');
                }
                ']' if !hash && brackets == 0 && !escape => {
                    parentheses -= 1;
                    buffer.push(']');
                }
                ' ' if !hash && parentheses == 0 && brackets == 0 && !escape && braces == 0 => {
                    if !buffer.is_empty() {
                        syntax.push(buffer.clone());
                        buffer.clear();
                    }
                }
                _ => {
                    if parentheses == 0 && brackets == 0 && !hash {
                        if escape {
                            match c {
                                'n' => buffer.push_str("\\n"),
                                't' => buffer.push_str("\\t"),
                                'r' => buffer.push_str("\\r"),
                                _ => buffer.push(c),
                            }
                        } else {
                            buffer.push(c);
                        }
                    } else {
                        if escape {
                            buffer.push('\\');
                        }
                        buffer.push(c);
                    }
                    escape = false; // Reset escape flag for non-escape characters
                }
            }
        }
        if !buffer.is_empty() {
            syntax.push(buffer);
        }
        syntax
    }

    /// evaluate string as program
    fn evaluate_program(&mut self, code: String) {
        // Parse into token string
        let syntax: Vec<String> = self.analyze_syntax(code);

        for token in syntax {
            // Show inside stack to debug
            let stack = self.show_stack();
            self.log_print(format!("{stack} ←  {token}\n"));

            // Character vector for token processing
            let chars: Vec<char> = token.chars().collect();

            // Judge what the token is
            if let Ok(i) = token.parse::<f64>() {
                // Push number value on the stack
                self.stack.push(Type::Number(i));
            } else if token == "true" || token == "false" {
                // Push bool value on the stack
                self.stack.push(Type::Bool(token.parse().unwrap_or(true)));
            } else if chars[0] == '(' && chars[chars.len() - 1] == ')' {
                // Processing string escape
                let string = {
                    let mut buffer = String::new(); // Temporary storage
                    let mut brackets = 0; // String's nest structure
                    let mut parentheses = 0; // List's nest structure
                    let mut hash = false; // Is it Comment
                    let mut escape = false; // Flag to indicate next character is escaped

                    for c in token[1..token.len() - 1].to_string().chars() {
                        match c {
                            '\\' if !escape => {
                                escape = true;
                            }
                            '(' if !hash && !escape => {
                                brackets += 1;
                                buffer.push('(');
                            }
                            ')' if !hash && !escape => {
                                brackets -= 1;
                                buffer.push(')');
                            }
                            '#' if !hash && !escape => {
                                hash = true;
                                buffer.push('#');
                            }
                            '#' if hash && !escape => {
                                hash = false;
                                buffer.push('#');
                            }
                            '[' if !hash && brackets == 0 && !escape => {
                                parentheses += 1;
                                buffer.push('[');
                            }
                            ']' if !hash && brackets == 0 && !escape => {
                                parentheses -= 1;
                                buffer.push(']');
                            }
                            _ => {
                                if parentheses == 0 && brackets == 0 && !hash {
                                    if escape {
                                        match c {
                                            'n' => buffer.push_str("\\n"),
                                            't' => buffer.push_str("\\t"),
                                            'r' => buffer.push_str("\\r"),
                                            _ => buffer.push(c),
                                        }
                                    } else {
                                        buffer.push(c);
                                    }
                                } else {
                                    if escape {
                                        buffer.push('\\');
                                    }
                                    buffer.push(c);
                                }
                                escape = false; // Reset escape flag for non-escape characters
                            }
                        }
                    }
                    buffer
                }; // Push string value on the stack
                self.stack.push(Type::String(string));
            } else if chars[0] == '[' && chars[chars.len() - 1] == ']' {
                // Push list value on the stack
                let old_len = self.stack.len(); // length of old stack
                let slice = &token[1..token.len() - 1];
                self.evaluate_program(slice.to_string());
                // Make increment of stack an element of list
                let mut list = Vec::new();
                for _ in old_len..self.stack.len() {
                    list.push(self.pop_stack());
                }
                list.reverse(); // reverse list
                self.stack.push(Type::List(list));
            } else if chars[0] == '{' && chars[chars.len() - 1] == '}' {
                let text = token[1..token.len() - 1].to_string();

                let row = text.split(";").collect::<Vec<&str>>().len();
                let col = text.split(";").collect::<Vec<&str>>()[0]
                    .split(",")
                    .collect::<Vec<&str>>()
                    .len();

                let value = text
                    .split(|c| c == ',' || c == ';')
                    .map(|x| {
                        self.evaluate_program(x.to_string());
                        self.pop_stack().get_number()
                    })
                    .collect::<Vec<f64>>();
                self.stack.push(Type::Matrix(value, (row, col)))
            } else if token.starts_with("error:") {
                // Push error value on the stack
                self.stack.push(Type::Error(token.replace("error:", "")))
            } else if let Some(i) = self.memory.get(&token) {
                // Push variable's data on stack
                self.stack.push(i.clone());
            } else if chars[0] == '#' && chars[chars.len() - 1] == '#' {
                // Processing comments
                self.log_print(format!("* Comment \"{}\"\n", token.replace('#', "")));
            } else {
                // Else, execute as command
                self.execute_command(token);
            }
        }

        // Show inside stack, after execution
        let stack = self.show_stack();
        self.log_print(format!("{stack}\n"));
    }

    /// execute string as commands
    fn execute_command(&mut self, command: String) {
        match command.as_str() {
            // Commands of calculation

            // Addition
            "add" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a + b));
            }

            // Subtraction
            "sub" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a - b));
            }

            // Multiplication
            "mul" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a * b));
            }

            // Division
            "div" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a / b));
            }

            // Remainder of division
            "mod" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a % b));
            }

            // Exponentiation
            "pow" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a.powf(b)));
            }

            // Rounding off
            "round" => {
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Number(a.round()));
            }

            // Trigonometric sine
            "sin" => {
                let number = self.pop_stack().get_number();
                self.stack.push(Type::Number(number.sin()))
            }

            // Trigonometric cosine
            "cos" => {
                let number = self.pop_stack().get_number();
                self.stack.push(Type::Number(number.cos()))
            }

            // Trigonometric tangent
            "tan" => {
                let number = self.pop_stack().get_number();
                self.stack.push(Type::Number(number.tan()))
            }

            // Logical operations of AND
            "and" => {
                let b = self.pop_stack().get_bool();
                let a = self.pop_stack().get_bool();
                self.stack.push(Type::Bool(a && b));
            }

            // Logical operations of OR
            "or" => {
                let b = self.pop_stack().get_bool();
                let a = self.pop_stack().get_bool();
                self.stack.push(Type::Bool(a || b));
            }

            // Logical operations of NOT
            "not" => {
                let b = self.pop_stack().get_bool();
                self.stack.push(Type::Bool(!b));
            }

            // Judge is it equal
            "equal" => {
                let b = self.pop_stack().get_string();
                let a = self.pop_stack().get_string();
                self.stack.push(Type::Bool(a == b));
            }

            // Judge is it less
            "less" => {
                let b = self.pop_stack().get_number();
                let a = self.pop_stack().get_number();
                self.stack.push(Type::Bool(a < b));
            }

            // Get random value from list
            "rand" => {
                let list = self.pop_stack().get_list();
                let result = match list.choose(&mut rand::thread_rng()) {
                    Some(i) => i.to_owned(),
                    None => Type::List(list),
                };
                self.stack.push(result);
            }

            // Shuffle list by random
            "shuffle" => {
                let mut list = self.pop_stack().get_list();
                list.shuffle(&mut rand::thread_rng());
                self.stack.push(Type::List(list));
            }

            // Commands of string processing

            // Repeat string a number of times
            "repeat" => {
                let count = self.pop_stack().get_number(); // Count
                let text = self.pop_stack().get_string(); // String
                self.stack.push(Type::String(text.repeat(count as usize)));
            }

            // Get unicode character form number
            "decode" => {
                let code = self.pop_stack().get_number();
                let result = char::from_u32(code as u32);
                match result {
                    Some(c) => self.stack.push(Type::String(c.to_string())),
                    None => {
                        self.log_print("Error! failed of number decoding\n".to_string());
                        self.stack.push(Type::Error("number-decoding".to_string()));
                    }
                }
            }

            // Encode string by UTF-8
            "encode" => {
                let string = self.pop_stack().get_string();
                if let Some(first_char) = string.chars().next() {
                    self.stack.push(Type::Number((first_char as u32) as f64));
                } else {
                    self.log_print("Error! failed of string encoding\n".to_string());
                    self.stack.push(Type::Error("string-encoding".to_string()));
                }
            }

            // Concatenate the string
            "concat" => {
                let b = self.pop_stack().get_string();
                let a = self.pop_stack().get_string();
                self.stack.push(Type::String(a + &b));
            }

            // Replacing string
            "replace" => {
                let after = self.pop_stack().get_string();
                let before = self.pop_stack().get_string();
                let text = self.pop_stack().get_string();
                self.stack.push(Type::String(text.replace(&before, &after)))
            }

            // Split string by the key
            "split" => {
                let key = self.pop_stack().get_string();
                let text = self.pop_stack().get_string();
                self.stack.push(Type::List(
                    text.split(&key)
                        .map(|x| Type::String(x.to_string()))
                        .collect::<Vec<Type>>(),
                ));
            }

            // Change string style case
            "case" => {
                let types = self.pop_stack().get_string();
                let text = self.pop_stack().get_string();

                self.stack.push(Type::String(match types.as_str() {
                    "lower" => text.to_lowercase(),
                    "upper" => text.to_uppercase(),
                    _ => text,
                }));
            }

            // Generate a string by concat list
            "join" => {
                let key = self.pop_stack().get_string();
                let mut list = self.pop_stack().get_list();
                self.stack.push(Type::String(
                    list.iter_mut()
                        .map(|x| x.get_string())
                        .collect::<Vec<String>>()
                        .join(&key),
                ))
            }

            // Judge is it find in string
            "find" => {
                let word = self.pop_stack().get_string();
                let text = self.pop_stack().get_string();
                self.stack.push(Type::Bool(text.contains(&word)))
            }

            // Search by regular expression
            "regex" => {
                let pattern = self.pop_stack().get_string();
                let text = self.pop_stack().get_string();

                let pattern: Regex = match Regex::new(pattern.as_str()) {
                    Ok(i) => i,
                    Err(e) => {
                        self.log_print(format!("Error! {}\n", e.to_string().replace("Error", "")));
                        self.stack.push(Type::Error("regex".to_string()));
                        return;
                    }
                };

                let mut list: Vec<Type> = Vec::new();
                for i in pattern.captures_iter(text.as_str()) {
                    list.push(Type::String(i[0].to_string()))
                }
                self.stack.push(Type::List(list));
            }

            // Commands of I/O

            // Write string in the file
            "write-file" => {
                let mut file = match File::create(Path::new(&self.pop_stack().get_string())) {
                    Ok(file) => file,
                    Err(e) => {
                        self.log_print(format!("Error! {e}\n"));
                        self.stack.push(Type::Error("create-file".to_string()));
                        return;
                    }
                };
                if let Err(e) = file.write_all(self.pop_stack().get_string().as_bytes()) {
                    self.log_print(format!("Error! {}\n", e));
                    self.stack.push(Type::Error("write-file".to_string()));
                }
            }

            // Read string in the file
            "read-file" => {
                let name = Path::new(&self.pop_stack().get_string()).to_owned();
                match get_file_contents(&name) {
                    Ok(s) => self.stack.push(Type::String(s)),
                    Err(e) => {
                        self.log_print(format!("Error! {}\n", e));
                        self.stack.push(Type::Error("read-file".to_string()));
                    }
                };
            }

            "read-binary" => {
                fn read_binary_file(path: String) -> io::Result<Vec<u8>> {
                    let file = File::open(Path::new(&path))?;
                    let mut buf_reader = BufReader::new(file);
                    let mut buffer = Vec::new();
                    buf_reader.read_to_end(&mut buffer)?;
                    Ok(buffer)
                }

                let binary = if let Ok(i) = read_binary_file(self.pop_stack().get_string()) {
                    i
                } else {
                    self.stack.push(Type::Error("read-binary".to_string()));
                    return;
                };

                self.stack.push(Type::Binary(binary));
            }

            // Standard input
            "input" => {
                let prompt = self.pop_stack().get_string();
                self.stack.push(Type::String(input(prompt.as_str())));
            }

            // Standard output
            "print" => {
                let a = self.pop_stack().get_string();

                let a = a.replace("\\n", "\n");
                let a = a.replace("\\t", "\t");
                let a = a.replace("\\r", "\r");

                if let Mode::Debug = self.mode {
                    println!("[Output]: {a}");
                } else {
                    print!("{a}");
                }
            }

            // Standard output with new line
            "println" => {
                let a = self.pop_stack().get_string();

                let a = a.replace("\\n", "\n");
                let a = a.replace("\\t", "\t");
                let a = a.replace("\\r", "\r");

                if let Mode::Debug = self.mode {
                    println!("[Output]: {a}");
                } else {
                    println!("{a}");
                }
            }

            // Get command-line arguments
            "args-cmd" => self.stack.push(Type::List(
                env::args()
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|x| Type::String(x.to_string()))
                    .collect::<Vec<Type>>(),
            )),

            // Play sound from frequency
            "play-sound" => {
                fn play_sine_wave(frequency: f64, duration_secs: f64) {
                    let sample_rate = 44100f64;

                    let num_samples = (duration_secs * sample_rate) as usize;
                    let samples: Vec<f32> = (0..num_samples)
                        .map(|t| {
                            let t = t as f64 / sample_rate;
                            (t * frequency * 2.0 * std::f64::consts::PI).sin() as f32
                        })
                        .collect();

                    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
                    let sink = Sink::try_new(&stream_handle).unwrap();

                    for _ in samples {
                        sink.append(
                            rodio::source::SineWave::new(frequency as f32)
                                .take_duration(Duration::from_secs_f64(duration_secs)),
                        );
                    }

                    sink.play();
                    std::thread::sleep(Duration::from_secs_f64(duration_secs));
                }

                let duration_secs = self.pop_stack().get_number();
                let frequency = self.pop_stack().get_number();

                play_sine_wave(frequency, duration_secs);
            }

            // Play the music file
            "play-file" => {
                let path = self.pop_stack().get_string();
                let sound_file_path = Path::new(&path);

                let res_sound_file = File::open(sound_file_path);

                if let Err(e) = res_sound_file {
                    self.log_print(format!("Error! {}\n", e));
                    self.stack.push(Type::Error("play-file".to_string()));
                } else {
                    let mut audio_device = Audio::new();
                    audio_device.add("sound", path.clone());
                    audio_device.play("sound");
                    audio_device.wait();

                    self.stack.push(Type::String(path));
                }
            }

            // Claer the console screen
            "cls" | "clear" => {
                let result = clearscreen::clear();
                if result.is_err() {
                    println!("Error! Failed to clear screen");
                    self.stack
                        .push(Type::Error(String::from("failed-to-clear-screen")));
                }
            }

            // Commands of control

            // Evaluate string as program
            "eval" => {
                let code = self.pop_stack().get_string();
                self.evaluate_program(code)
            }

            // Conditional branch
            "if" => {
                let condition = self.pop_stack().get_bool(); // Condition
                let code_else = self.pop_stack().get_string(); // Code of else
                let code_if = self.pop_stack().get_string(); // Code of If
                if condition {
                    self.evaluate_program(code_if)
                } else {
                    self.evaluate_program(code_else)
                };
            }

            // Loop while condition is true
            "while" => {
                let cond = self.pop_stack().get_string();
                let code = self.pop_stack().get_string();
                while {
                    self.evaluate_program(cond.clone());
                    self.pop_stack().get_bool()
                } {
                    self.evaluate_program(code.clone());
                }
            }

            // Generate a thread
            "thread" => {
                let code = self.pop_stack().get_string();
                let mut executor = self.clone();
                thread::spawn(move || executor.evaluate_program(code));
            }

            // Exit a process
            "exit" => {
                let status = self.pop_stack().get_number();
                std::process::exit(status as i32);
            }

            // Commands of list processing

            // Get list value by index
            "get" => {
                let index = self.pop_stack().get_number() as usize;
                let list: Vec<Type> = self.pop_stack().get_list();
                if list.len() > index {
                    self.stack.push(list[index].clone());
                } else {
                    self.log_print("Error! Index specification is out of range\n".to_string());
                    self.stack.push(Type::Error("index-out-range".to_string()));
                }
            }

            // Set list value by index
            "set" => {
                let value = self.pop_stack();
                let index = self.pop_stack().get_number() as usize;
                let mut list: Vec<Type> = self.pop_stack().get_list();
                if list.len() > index {
                    list[index] = value;
                    self.stack.push(Type::List(list));
                } else {
                    self.log_print("Error! Index specification is out of range\n".to_string());
                    self.stack.push(Type::Error("index-out-range".to_string()));
                }
            }

            // Delete list value by index
            "del" => {
                let index = self.pop_stack().get_number() as usize;
                let mut list = self.pop_stack().get_list();
                if list.len() > index {
                    list.remove(index);
                    self.stack.push(Type::List(list));
                } else {
                    self.log_print("Error! Index specification is out of range\n".to_string());
                    self.stack.push(Type::Error("index-out-range".to_string()));
                }
            }

            // Append value in the list
            "append" => {
                let data = self.pop_stack();
                let mut list = self.pop_stack().get_list();
                list.push(data);
                self.stack.push(Type::List(list));
            }

            // Insert value in the list
            "insert" => {
                let data = self.pop_stack();
                let index = self.pop_stack().get_number();
                let mut list = self.pop_stack().get_list();
                list.insert(index as usize, data);
                self.stack.push(Type::List(list));
            }

            // Get index of the list
            "index" => {
                let target = self.pop_stack().get_string();
                let list = self.pop_stack().get_list();

                for (index, item) in list.iter().enumerate() {
                    if target == item.clone().get_string() {
                        self.stack.push(Type::Number(index as f64));
                        return;
                    }
                }
                self.log_print(String::from("Error! item not found in the list\n"));
                self.stack.push(Type::Error(String::from("item-not-found")));
            }

            // Sorting in the list
            "sort" => {
                let mut list: Vec<String> = self
                    .pop_stack()
                    .get_list()
                    .iter()
                    .map(|x| x.to_owned().get_string())
                    .collect();
                list.sort();
                self.stack.push(Type::List(
                    list.iter()
                        .map(|x| Type::String(x.to_string()))
                        .collect::<Vec<_>>(),
                ));
            }

            // reverse in the list
            "reverse" => {
                let mut list = self.pop_stack().get_list();
                list.reverse();
                self.stack.push(Type::List(list));
            }

            // Iteration for the list
            "for" => {
                let code = self.pop_stack().get_string();
                let vars = self.pop_stack().get_string();
                let list = self.pop_stack().get_list();

                list.iter().for_each(|x| {
                    self.memory
                        .entry(vars.clone())
                        .and_modify(|value| *value = x.clone())
                        .or_insert(x.clone());
                    self.evaluate_program(code.clone());
                });
            }

            // Generate a range
            "range" => {
                let step = self.pop_stack().get_number();
                let max = self.pop_stack().get_number();
                let min = self.pop_stack().get_number();

                let mut range: Vec<Type> = Vec::new();
                let mut i = min;

                while i < max {
                    range.push(Type::Number(i));
                    i += step;
                }

                self.stack.push(Type::List(range));
            }

            // Get length of list
            "len" => {
                let data = self.pop_stack().get_list();
                self.stack.push(Type::Number(data.len() as f64));
            }

            // Commands of functional programming

            // Mapping a list
            "map" => {
                let code = self.pop_stack().get_string();
                let vars = self.pop_stack().get_string();
                let list = self.pop_stack().get_list();

                let mut result_list = Vec::new();
                for x in list.iter() {
                    self.memory
                        .entry(vars.clone())
                        .and_modify(|value| *value = x.clone())
                        .or_insert(x.clone());

                    self.evaluate_program(code.clone());
                    result_list.push(self.pop_stack());
                }

                self.stack.push(Type::List(result_list));
            }

            // Filtering a list value
            "filter" => {
                let code = self.pop_stack().get_string();
                let vars = self.pop_stack().get_string();
                let list = self.pop_stack().get_list();

                let mut result_list = Vec::new();

                for x in list.iter() {
                    self.memory
                        .entry(vars.clone())
                        .and_modify(|value| *value = x.clone())
                        .or_insert(x.clone());

                    self.evaluate_program(code.clone());
                    if self.pop_stack().get_bool() {
                        result_list.push(x.clone());
                    }
                }

                self.stack.push(Type::List(result_list));
            }

            // Generate value from list
            "reduce" => {
                let code = self.pop_stack().get_string();
                let now = self.pop_stack().get_string();
                let init = self.pop_stack();
                let acc = self.pop_stack().get_string();
                let list = self.pop_stack().get_list();

                self.memory
                    .entry(acc.clone())
                    .and_modify(|value| *value = init.clone())
                    .or_insert(init);

                for x in list.iter() {
                    self.memory
                        .entry(now.clone())
                        .and_modify(|value| *value = x.clone())
                        .or_insert(x.clone());

                    self.evaluate_program(code.clone());
                    let result = self.pop_stack();

                    self.memory
                        .entry(acc.clone())
                        .and_modify(|value| *value = result.clone())
                        .or_insert(result);
                }

                let result = self.memory.get(&acc);
                self.stack
                    .push(result.unwrap_or(&Type::String("".to_string())).clone());

                self.memory
                    .entry(acc.clone())
                    .and_modify(|value| *value = Type::String("".to_string()))
                    .or_insert(Type::String("".to_string()));
            }

            // Commands of memory manage

            // Pop in the stack
            "pop" => {
                self.pop_stack();
            }

            // Get size of stack
            "size-stack" => {
                let len: f64 = self.stack.len() as f64;
                self.stack.push(Type::Number(len));
            }

            // Get Stack as List
            "get-stack" => {
                self.stack.push(Type::List(self.stack.clone()));
            }

            // Define variable at memory
            "var" => {
                let name = self.pop_stack().get_string();
                let data = self.pop_stack();
                self.memory
                    .entry(name)
                    .and_modify(|value| *value = data.clone())
                    .or_insert(data);
                self.show_variables()
            }

            // Get data type of value
            "type" => {
                let result = match self.pop_stack() {
                    Type::Number(_) => "number".to_string(),
                    Type::String(_) => "string".to_string(),
                    Type::Bool(_) => "bool".to_string(),
                    Type::List(_) => "list".to_string(),
                    Type::Error(_) => "error".to_string(),
                    Type::Matrix(_, _) => "matrix".to_string(),
                    Type::Binary(_) => "binary".to_string(),
                    Type::Json(_) => "json".to_string(),
                    Type::Object(name, _) => name.to_string(),
                    Type::Image(_) => "image".to_string(),
                };

                self.stack.push(Type::String(result));
            }

            // Explicit data type casting
            "cast" => {
                let types = self.pop_stack().get_string();
                let value = self.pop_stack();
                match types.as_str() {
                    "number" => self.stack.push(Type::Number(value.get_number())),
                    "string" => self.stack.push(Type::String(value.get_string())),
                    "bool" => self.stack.push(Type::Bool(value.get_bool())),
                    "list" => self.stack.push(Type::List(value.get_list())),
                    "json" => self.stack.push(Type::Json(value.get_json())),
                    "error" => self.stack.push(Type::Error(value.get_string())),
                    _ => self.stack.push(value),
                }
            }

            // Get memory information
            "mem" => {
                let mut list: Vec<Type> = Vec::new();
                for (name, _) in self.memory.clone() {
                    list.push(Type::String(name))
                }
                self.stack.push(Type::List(list))
            }

            // Free up memory space of variable
            "free" => {
                let name = self.pop_stack().get_string();
                self.memory.remove(name.as_str());
                self.show_variables();
            }

            // Copy stack's top value
            "copy" => {
                let data = self.pop_stack();
                self.stack.push(data.clone());
                self.stack.push(data);
            }

            // Swap stack's top 2 value
            "swap" => {
                let b = self.pop_stack();
                let a = self.pop_stack();
                self.stack.push(b);
                self.stack.push(a);
            }

            // Commands of times

            // Get now time as unix epoch
            "now-time" => {
                self.stack.push(Type::Number(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64(),
                ));
            }

            // Sleep fixed time
            "sleep" => sleep(Duration::from_secs_f64(self.pop_stack().get_number())),

            // Commands of object oriented system

            // Generate a instance of object
            "instance" => {
                let data = self.pop_stack().get_list();
                let class = self.pop_stack().get_list();
                let mut object: HashMap<String, Type> = HashMap::new();

                let name = if !class.is_empty() {
                    class[0].get_string()
                } else {
                    self.log_print("Error! the type name is not found.".to_string());
                    self.stack.push(Type::Error("instance-name".to_string()));
                    return;
                };

                let mut index = 0;
                for item in &mut class.to_owned()[1..class.len()].iter() {
                    let item = item.to_owned();
                    if item.get_list().len() == 1 {
                        let element = match data.get(index) {
                            Some(value) => value,
                            None => {
                                self.log_print("Error! initial data is shortage\n".to_string());
                                self.stack
                                    .push(Type::Error("instance-shortage".to_string()));
                                return;
                            }
                        };
                        object.insert(
                            item.get_list()[0].to_owned().get_string(),
                            element.to_owned(),
                        );
                        index += 1;
                    } else if item.get_list().len() >= 2 {
                        let item = item.get_list();
                        object.insert(item[0].clone().get_string(), item[1].clone());
                    } else {
                        self.log_print("Error! the class data structure is wrong.".to_string());
                        self.stack.push(Type::Error("instance-default".to_string()));
                    }
                }

                self.stack.push(Type::Object(name, object))
            }

            // Get property of object
            "property" => {
                let name = self.pop_stack().get_string();
                let (_, object) = self.pop_stack().get_object();
                self.stack.push(
                    object
                        .get(name.as_str())
                        .unwrap_or(&Type::Error("property".to_string()))
                        .clone(),
                )
            }

            // Call the method of object
            "method" => {
                let method = self.pop_stack().get_string();
                let (name, value) = self.pop_stack().get_object();
                let data = Type::Object(name, value.clone());
                self.memory
                    .entry("self".to_string())
                    .and_modify(|value| *value = data.clone())
                    .or_insert(data);

                let program: String = match value.get(&method) {
                    Some(i) => i.to_owned().get_string().to_string(),
                    None => "".to_string(),
                };

                self.evaluate_program(program);
            }

            // Modify the property of object
            "modify" => {
                let data = self.pop_stack();
                let property = self.pop_stack().get_string();
                let (name, mut value) = self.pop_stack().get_object();
                value
                    .entry(property)
                    .and_modify(|value| *value = data.clone())
                    .or_insert(data.clone());

                self.stack.push(Type::Object(name, value))
            }

            // Get all of properties
            "all" => {
                let (_, value) = self.pop_stack().get_object();
                self.stack.push(Type::List(
                    value
                        .keys()
                        .map(|x| Type::String(x.to_owned()))
                        .collect::<Vec<Type>>(),
                ));
            }

            // Commands of external cooperation processing

            // Send the http request
            "request" => {
                let url = self.pop_stack().get_string();
                match reqwest::blocking::get(url) {
                    Ok(i) => self
                        .stack
                        .push(Type::String(i.text().unwrap_or("".to_string()))),
                    Err(e) => {
                        self.log_print(format!("Error! {e}\n"));
                        self.stack.push(Type::Error("request".to_string()))
                    }
                }
            }

            // Open the file or url
            "open" => {
                let name = self.pop_stack().get_string();
                if let Err(e) = opener::open(name.clone()) {
                    self.log_print(format!("Error! {e}\n"));
                    self.stack.push(Type::Error("open".to_string()));
                } else {
                    self.stack.push(Type::String(name))
                }
            }

            // Change current directory
            "cd" => {
                let name = self.pop_stack().get_string();
                if let Err(err) = std::env::set_current_dir(name.clone()) {
                    self.log_print(format!("Error! {}\n", err));
                    self.stack.push(Type::Error("cd".to_string()));
                } else {
                    self.stack.push(Type::String(name))
                }
            }

            // Get current directory
            "pwd" => {
                if let Ok(current_dir) = std::env::current_dir() {
                    if let Some(path) = current_dir.to_str() {
                        self.stack.push(Type::String(String::from(path)));
                    }
                }
            }

            // Make directory
            "mkdir" => {
                let name = self.pop_stack().get_string();
                if let Err(e) = fs::create_dir(name.clone()) {
                    self.log_print(format!("Error! {e}\n"));
                    self.stack.push(Type::Error("mkdir".to_string()));
                } else {
                    self.stack.push(Type::String(name))
                }
            }

            // Remove item
            "rm" => {
                let name = self.pop_stack().get_string();
                if Path::new(name.as_str()).is_dir() {
                    if let Err(e) = fs::remove_dir(name.clone()) {
                        self.log_print(format!("Error! {e}\n"));
                        self.stack.push(Type::Error("rm".to_string()));
                    } else {
                        self.stack.push(Type::String(name))
                    }
                } else if let Err(e) = fs::remove_file(name.clone()) {
                    self.log_print(format!("Error! {e}\n"));
                    self.stack.push(Type::Error("rm".to_string()));
                } else {
                    self.stack.push(Type::String(name))
                }
            }

            // Rename item
            "rename" => {
                let to = self.pop_stack().get_string();
                let from = self.pop_stack().get_string();
                if let Err(e) = fs::rename(from, to.clone()) {
                    self.log_print(format!("Error! {e}\n"));
                    self.stack.push(Type::Error("rename".to_string()));
                } else {
                    self.stack.push(Type::String(to))
                }
            }

            // Copy the item
            "cp" => {
                let to = self.pop_stack().get_string();
                let from = self.pop_stack().get_string();

                match fs::copy(from, to) {
                    Ok(i) => self.stack.push(Type::Number(i as f64)),
                    Err(e) => {
                        self.log_print(format!("Error! {e}\n"));
                        self.stack.push(Type::Error("cp".to_string()))
                    }
                }
            }

            // Get size of the file
            "size-file" => match fs::metadata(self.pop_stack().get_string()) {
                Ok(i) => self.stack.push(Type::Number(i.len() as f64)),
                Err(e) => {
                    self.log_print(format!("Error! {e}\n"));
                    self.stack.push(Type::Error("size-file".to_string()))
                }
            },

            // Get list of files
            "ls" => {
                if let Ok(entries) = fs::read_dir(".") {
                    let value: Vec<Type> = entries
                        .filter_map(|entry| {
                            entry
                                .ok()
                                .and_then(|e| e.file_name().into_string().ok())
                                .map(Type::String)
                        })
                        .collect();
                    self.stack.push(Type::List(value));
                }
            }

            // Judge is it folder
            "folder" => {
                let path = self.pop_stack().get_string();
                let path = Path::new(path.as_str());
                self.stack.push(Type::Bool(path.is_dir()));
            }

            // Get system information
            "sys-info" => {
                let option = self.pop_stack().get_string();
                self.stack.push(match option.as_str() {
                    "os-release" => Type::String(os_release().unwrap_or("".to_string())),
                    "os-type" => Type::String(os_type().unwrap_or("".to_string())),
                    "cpu-num" => Type::Number(cpu_num().unwrap_or(0) as f64),
                    "cpu-speed" => Type::Number(cpu_speed().unwrap_or(0) as f64),
                    "host-name" => Type::String(hostname().unwrap_or("".to_string())),
                    "mem-size" => match mem_info() {
                        Ok(info) => Type::Number(info.total as f64),
                        Err(_) => Type::Error("sys-info".to_string()),
                    },
                    "mem-used" => match mem_info() {
                        Ok(info) => Type::Number((info.total - info.free) as f64),
                        Err(_) => Type::Error("sys-info".to_string()),
                    },
                    _ => Type::Error("sys-info".to_string()),
                })
            }

            // Set value in the clipboard
            "set-clipboard" => {
                let mut ctx: ClipboardContext;
                if let Ok(i) = ClipboardProvider::new() {
                    ctx = i
                } else {
                    self.stack.push(Type::Error("set-clipboard".to_string()));
                    return;
                };

                let value = self.pop_stack().get_string();
                if ctx.set_contents(value.clone()).is_ok() {
                    self.stack.push(Type::String(value));
                } else {
                    self.stack.push(Type::Error("set-clipboard".to_string()))
                };
            }

            // Get value in the clipboard
            "get-clipboard" => {
                let mut ctx: ClipboardContext;
                if let Ok(i) = ClipboardProvider::new() {
                    ctx = i
                } else {
                    self.stack.push(Type::Error("get-clipboard".to_string()));
                    return;
                };

                if let Ok(contents) = ctx.get_contents() {
                    self.stack.push(Type::String(contents));
                } else {
                    self.stack.push(Type::Error("get-clipboard".to_string()))
                }
            }

            // Commands of web server

            // Get value from json
            "get-json" => {
                let key = self.pop_stack().get_string();
                let json = self.pop_stack().get_json();
                self.stack.push(Type::Json(json[key].clone()))
            }

            // Set value of json
            "set-json" => {
                let value = self.pop_stack().get_json();
                let key = self.pop_stack().get_string();
                let mut json = self.pop_stack().get_json();
                json[key] = value;
                self.stack.push(Type::Json(json))
            }

            // Control SQL
            "sql" => {
                let path = self.pop_stack().get_string();
                let query = self.pop_stack().get_string();
                self.stack.push(sql(&path, &query));
            }

            // Templates processing by jinja2
            "template" => {
                let mut tera = Tera::default();

                // Get render value from object
                let render_object = if let Type::Object(_, obj) = self.pop_stack() {
                    obj
                } else {
                    self.stack.push(Type::Error("not-object".to_string()));
                    return;
                };
                let template_string = self.pop_stack().get_string();

                let mut context = Context::new();

                // Parse type from object to context
                for (key, value) in render_object {
                    context.insert(key, &value.get_string())
                }

                // rendering string
                let rendered = tera.render_str(&template_string, &context).unwrap();
                self.stack.push(Type::String(rendered));
            }

            // start web server
            "start-server" => {
                let code: Type = self.pop_stack();
                let option: Type = self.pop_stack();
                self.server(option, code);
            }

            // Commands of matrix
            "scalar-mul" => {
                let number = self.pop_stack().get_number();

                let (matrix, (rows, cols)) = self.pop_stack().get_matrix();

                let matrix = nalgebra::DMatrix::from_row_slice(rows, cols, &matrix);
                let result: Vec<f64> = (matrix * number).iter().cloned().collect();

                self.stack.push(Type::Matrix(
                    result.iter().map(|x| *x).collect(),
                    (rows, cols),
                ))
            }

            "add-matrix" => {
                let (matrix1, (rows1, cols1)) = self.pop_stack().get_matrix();
                let matrix1 = nalgebra::DMatrix::from_row_slice(rows1, cols1, &matrix1);

                let (matrix2, (rows2, cols2)) = self.pop_stack().get_matrix();
                let matrix2 = nalgebra::DMatrix::from_row_slice(rows2, cols2, &matrix2);

                let result: Vec<f64> = (matrix1 + matrix2).iter().cloned().collect();

                self.stack.push(Type::Matrix(
                    result.iter().map(|x| *x).collect(),
                    (rows1, cols1),
                ))
            }

            "sub-matrix" => {
                let (matrix1, (rows1, cols1)) = self.pop_stack().get_matrix();
                let matrix1 = nalgebra::DMatrix::from_row_slice(rows1, cols1, &matrix1);

                let (matrix2, (rows2, cols2)) = self.pop_stack().get_matrix();
                let matrix2 = nalgebra::DMatrix::from_row_slice(rows2, cols2, &matrix2);

                let result: Vec<f64> = (matrix2 - matrix1).iter().cloned().collect();

                self.stack.push(Type::Matrix(
                    result.iter().map(|x| *x).collect(),
                    (rows1, cols1),
                ))
            }

            "mul-matrix" => {
                let (matrix1, (rows1, cols1)) = self.pop_stack().get_matrix();
                let matrix1 = nalgebra::DMatrix::from_row_slice(rows1, cols1, &matrix1);

                let (matrix2, (rows2, cols2)) = self.pop_stack().get_matrix();
                let matrix2 = nalgebra::DMatrix::from_row_slice(rows2, cols2, &matrix2);

                let result: Vec<f64> = (matrix1 * matrix2).iter().cloned().collect();

                self.stack.push(Type::Matrix(
                    result.iter().map(|x| *x).collect(),
                    (rows1, cols1),
                ))
            }

            "transpose" => {
                let (matrix, (rows, cols)) = self.pop_stack().get_matrix();
                let matrix = nalgebra::DMatrix::from_row_slice(rows, cols, &matrix);
                let transposed_matrix = matrix.transpose();

                let mut transposed_data = Vec::new();
                for i in 0..transposed_matrix.nrows() {
                    for j in 0..transposed_matrix.ncols() {
                        transposed_data.push(transposed_matrix[(i, j)]);
                    }
                }

                self.stack.push(Type::Matrix(transposed_data, (cols, rows)))
            }

            "graph" => {
                let (data, (row, col)) = self.pop_stack().get_matrix();
                let adjacency_matrix = nalgebra::DMatrix::<f64>::from_row_slice(row, col, &data);

                let mut graph = Graph::<f64, ()>::new();
                let mut node_indices = Vec::new();

                for &value in &data {
                    let node_index = graph.add_node(value); // ノードのラベルに値を設定
                    node_indices.push(node_index);
                }
                for (i, row) in adjacency_matrix.row_iter().enumerate() {
                    for (j, &value) in row.iter().enumerate() {
                        if value != 0.0 {
                            graph.add_edge(node_indices[i], node_indices[j], ());
                        }
                    }
                }
                let dot = format!("{:?}", Dot::with_config(&graph, &[Config::EdgeNoLabel]));
                self.stack.push(Type::String(dot))
            }

            // Commands of OpenCV image processing

            // Open image file
            "open-image" => {
                let image_path: &str = &self.pop_stack().get_string();
                self.stack.push(Type::Image(
                    imgcodecs::imread(image_path, imgcodecs::IMREAD_COLOR).unwrap(),
                ))
            }

            // Show image using GUI window
            "show-image" => {
                //Display the image
                let window_name: &str = "Image Window";
                highgui::named_window(window_name, highgui::WINDOW_NORMAL).unwrap();
                highgui::imshow(window_name, &self.pop_stack().get_image()).unwrap();

                // Wait for a key press
                highgui::wait_key(0).unwrap();
            }

            // Modify image to grayscale
            "to-grayscale" => {
                fn to_grayscale(img: &Mat) -> Mat {
                    let mut gray_img = Mat::default();
                    imgproc::cvt_color(img, &mut gray_img, imgproc::COLOR_BGR2GRAY, 0).unwrap();
                    gray_img
                }

                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(to_grayscale(img)))
            }

            // Modify image to invert its color
            "invert-color" => {
                fn invert_color(img: &Mat) -> Mat {
                    let mut inverted_img = Mat::default();
                    core::bitwise_not(img, &mut inverted_img, &core::no_array()).unwrap();

                    inverted_img
                }

                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(invert_color(img)))
            }

            // Modify image to flip vertical or horizontal
            "flip-image" => {
                fn flip(img: &Mat, direction: i32) -> Mat {
                    let mut flipped_img = Mat::default();
                    core::flip(img, &mut flipped_img, direction).unwrap();
                    flipped_img
                }

                let direction = self.pop_stack().get_string();
                let direction = if direction == "vertical" {
                    0
                } else if direction == "horizontal" {
                    1
                } else {
                    self.stack.push(Type::Error("flip-image".to_string()));
                    return;
                };
                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(flip(img, direction)))
            }

            // Modify image to blur using gaussian
            "gaussian-blur" => {
                fn gaussian_blur(img: &Mat, ksize: i32) -> Mat {
                    let mut blurred_img = Mat::default();
                    let ksize = core::Size::new(ksize, ksize);
                    imgproc::gaussian_blur(
                        img,
                        &mut blurred_img,
                        ksize,
                        0.0,
                        0.0,
                        core::BORDER_DEFAULT,
                    )
                    .unwrap();
                    blurred_img
                }

                let ksize = self.pop_stack().get_number();
                let img = &self.pop_stack().get_image();
                self.stack
                    .push(Type::Image(gaussian_blur(img, ksize as i32)))
            }

            // Modify image to resize its width and height
            "resize-image" => {
                fn resize_image(img: &Mat, width: i32, height: i32) -> Mat {
                    let mut resized_img = Mat::default();
                    resize(
                        img,
                        &mut resized_img,
                        core::Size::new(width, height),
                        0.0,
                        0.0,
                        0,
                    )
                    .unwrap();
                    resized_img
                }
                let height = self.pop_stack().get_number();
                let width = self.pop_stack().get_number();
                let img = &self.pop_stack().get_image();
                self.stack
                    .push(Type::Image(resize_image(img, width as i32, height as i32)))
            }

            // Detect edge of image
            "edge-detect" => {
                fn edge_detection(img: &Mat) -> Mat {
                    let mut gray_img = Mat::default();
                    let mut edges = Mat::default();
                    imgproc::cvt_color(img, &mut gray_img, imgproc::COLOR_BGR2GRAY, 0).unwrap();
                    imgproc::canny(&gray_img, &mut edges, 100.0, 200.0, 3, false).unwrap();
                    edges
                }
                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(edge_detection(img)))
            }

            // Modify image to mapping its color
            "color-map" => {
                fn apply_color_map(img: &Mat) -> Mat {
                    let mut color_img = Mat::default();
                    imgproc::apply_color_map(img, &mut color_img, imgproc::COLORMAP_JET).unwrap();
                    color_img
                }
                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(apply_color_map(img)))
            }

            // Modify image to morphology operation
            "morphology-operation" => {
                fn morphology_operation(img: &Mat, operation: i32, kernel_size: i32) -> Mat {
                    let mut result_img = Mat::default();
                    let kernel = imgproc::get_structuring_element(
                        imgproc::MORPH_RECT,
                        opencv::core::Size::new(kernel_size, kernel_size),
                        core::Point::new(-1, -1),
                    )
                    .unwrap();

                    imgproc::morphology_ex(
                        img,
                        &mut result_img,
                        operation,
                        &kernel,
                        core::Point::new(-1, -1),
                        1,
                        core::BORDER_CONSTANT,
                        core::Scalar::all(0.0),
                    )
                    .unwrap();

                    result_img
                }
                let kernel_size = self.pop_stack().get_number();
                let operation = match self.pop_stack().get_string().as_str() {
                    "dilate" => imgproc::MORPH_DILATE,
                    "erode" => imgproc::MORPH_ERODE,
                    "open" => imgproc::MORPH_OPEN,
                    "close" => imgproc::MORPH_CLOSE,
                    _ => {
                        self.stack
                            .push(Type::Error("morphology-operation".to_string()));
                        return;
                    }
                };
                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(morphology_operation(
                    img,
                    operation as i32,
                    kernel_size as i32,
                )))
            }

            // Modify image to histogram equalization
            "histogram-equalization" => {
                fn histogram_equalization(img: &Mat) -> Mat {
                    let mut equalized_img = Mat::default();
                    imgproc::equalize_hist(img, &mut equalized_img).unwrap();
                    equalized_img
                }

                let img = &self.pop_stack().get_image();
                self.stack.push(Type::Image(histogram_equalization(img)))
            }

            // Save image to file
            "save-image" => {
                let name = &self.pop_stack().get_string();
                let img = &self.pop_stack().get_image();
                opencv::imgcodecs::imwrite(name, &img, &core::Vector::new()).unwrap();
            }

            // Modify image to sharpe
            "to-sharpe" => {
                fn to_sharpe(img: Mat, level: f64) -> Mat {
                    let kernel = Mat::from_slice_2d(&[
                        [-1f64, -1f64, -1f64],
                        [-1f64, level, -1f64],
                        [-1f64, -1f64, -1f64],
                    ])
                    .unwrap();
                    let mut sharpened_img = Mat::default();
                    imgproc::filter_2d(
                        &img,
                        &mut sharpened_img,
                        -1,
                        &kernel,
                        core::Point::new(-1, -1),
                        0.0,
                        core::BORDER_DEFAULT,
                    )
                    .unwrap();
                    sharpened_img
                }
                let level = self.pop_stack().get_number();
                let img = self.pop_stack().get_image();
                self.stack.push(Type::Image(to_sharpe(img, level)))
            }

            // Commands of GUI processing

            // GUI processing
            "gui" => {
                let option = self.pop_stack();
                self.gui(option);
            }

            // Message box
            "msgbox" => {
                let (title, object) = self.pop_stack().get_object();

                MessageDialog::new()
                    .set_type(
                        match object
                            .get("type")
                            .unwrap_or(&Type::String("info".to_string()))
                            .get_string()
                            .as_str()
                        {
                            "error" => MessageType::Error,
                            "warning" => MessageType::Warning,
                            _ => MessageType::Info,
                        },
                    )
                    .set_title(&title)
                    .set_text(
                        &object
                            .get("text")
                            .unwrap_or(&Type::String("Hello, StackGUI !!!".to_string()))
                            .get_string(),
                    )
                    .show_alert()
                    .unwrap();
            }

            // If it is not recognized as a command, use it as a string.
            _ => self.stack.push(Type::String(command)),
        }
    }

    /// GUI window manager
    fn gui(&mut self, object: Type) {
        let (title, members): (String, HashMap<String, Type>) =
            if let Type::Object(title, members) = object.clone() {
                (title, members)
            } else {
                ("Hello".to_string(), HashMap::new())
            };

        let width = members
            .get("width")
            .unwrap_or(&Type::Number(800f64))
            .get_number() as i32;
        let height = members
            .get("height")
            .unwrap_or(&Type::Number(600f64))
            .get_number() as i32;

        let layout = members
            .get("layout")
            .unwrap_or(&Type::String("<h1>Hello, StackGUI !!!</h1>".to_string()))
            .get_string();

        web_view::builder()
            .title(&title.to_string())
            .content(Content::Html(layout))
            .size(width, height)
            .resizable(true)
            .debug(true)
            .user_data(())
            .invoke_handler(|webview, arg| {
                self.stack.push(Type::String(arg.to_string()));
                self.stack.push(object.clone());

                self.evaluate_program("(code) method".to_string());
                let _result = webview.eval(&self.pop_stack().get_string());
                Ok(())
            })
            .run()
            .unwrap();
    }

    /// Pop stack's top value
    fn pop_stack(&mut self) -> Type {
        if let Some(value) = self.stack.pop() {
            value
        } else {
            self.log_print(
                "Error! There are not enough values on the stack. returns default value\n"
                    .to_string(),
            );
            Type::String("".to_string())
        }
    }

    /// Http request handler
    fn handle(
        &mut self,
        mut stream: TcpStream,
        routes: HashMap<String, (String, bool, String)>,
        buffer_size: usize,
    ) {
        let mut buffer = vec![0; buffer_size];
        stream.read(&mut buffer).unwrap();

        let request_str = String::from_utf8_lossy(&buffer);
        let mut lines = request_str.lines();
        let request_line = lines.next().unwrap_or_default();
        let (method, path) = parse_request_line(request_line, " ");
        let (path, query) = parse_request_line(&path, "?");

        // Find the empty line separating headers and body
        while let Some(line) = lines.next() {
            if line.is_empty() {
                break;
            }
        }

        // Get request body
        let mut body = percent_decode_str(&query)
            .decode_utf8()
            .unwrap_or_default()
            .trim()
            .trim_end_matches(char::from(0))
            .to_string();

        while let Some(line) = lines.next() {
            if line.is_empty() {
                break;
            }
            body.push_str(
                &percent_decode_str(line)
                    .decode_utf8()
                    .unwrap_or_default()
                    .trim()
                    .trim_end_matches(char::from(0)),
            );
        }

        // Generate string to match handler option
        let matching = vec![method.to_string(), path.to_string()].join(" ");

        if let Some((code, auth, auth_data)) = routes.get(&matching).clone() {
            if *auth {
                let auth: &Type = &{
                    self.evaluate_program(auth_data.to_owned());
                    self.pop_stack()
                };

                // Generate user database
                let mut database: HashMap<String, String> = HashMap::new();
                for i in &mut auth.get_list() {
                    let i = i.get_list();
                    database.insert(i[0].get_string(), i[1].get_string());
                }

                let (is_auth, (user, pass)) = authenticate(&request_str, database);

                // Processing when fault to authenticate
                if !is_auth {
                    let response = "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"Restricted area\"\r\nContent-Type: text/plain\r\n\r\nUnauthorized".to_string();
                    stream.write(response.as_bytes()).unwrap();
                    stream.flush().unwrap();
                    return;
                }

                // Push user data on the stack
                let user_data = Type::List(vec![Type::String(user), Type::String(pass)]);
                self.stack.push(user_data);
            }

            let body = Type::String(body);

            // Push request body on the stack
            self.stack.push(body);

            self.evaluate_program(code.to_owned());

            let response_value = self.pop_stack();
            if let Type::Binary(i) = response_value.clone() {
                let value = [
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {};\r\n\r\n",
                        self.pop_stack().get_string()
                    )
                    .as_bytes(),
                    i.as_slice(),
                ]
                .as_slice()
                .concat();

                stream.write(&value).unwrap();
                stream.flush().unwrap();
            }
            stream
                .write(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {1}; charset=utf-8\r\n\r\n{0}",
                        response_value.get_string(),
                        self.pop_stack().get_string()
                    )
                    .as_bytes(),
                )
                .unwrap();
            stream.flush().unwrap();
        } else {
            // Processing when user access pages that not exist

            stream
                .write(
                    format!(
                        "HTTP/1.1 404 NOT FOUND\r\nContent-Type: {1}; charset=utf-8\r\n\r\n{0}",
                        if let Some((code, _, _)) = routes.get("not-found") {
                            self.evaluate_program(code.to_owned());
                            self.pop_stack().get_string()
                        } else {
                            "404 - Not found".to_string()
                        },
                        self.pop_stack().get_string()
                    )
                    .as_bytes(),
                )
                .unwrap();
            stream.flush().unwrap();
        };
    }

    // Main web server function
    fn server(&mut self, option: Type, code: Type) {
        let (name, address, buffer_size): (String, String, usize) =
            if let Type::Object(name, value) = option {
                (
                    name,
                    value
                        .get("address")
                        .unwrap_or(&Type::String("127.0.0.1:8000".to_string()))
                        .get_string(),
                    value
                        .get("buffer-size")
                        .unwrap_or(&Type::Number(8192f64))
                        .get_number() as usize,
                )
            } else {
                ("app".to_string(), (option.get_string()), 8192)
            };

        let listener = TcpListener::bind(address.clone()).unwrap();
        println!("Server '{name}' is started on http://{address}");
        println!("The request body's acceptable buffer size is {buffer_size} bytes");

        // Get route handler options in the Stack code
        let mut hashmap: HashMap<String, (String, bool, String)> = HashMap::new();
        for i in code.get_list() {
            let matching = i.get_list()[0].get_list();
            let route = matching[0].get_string();
            let is_auth: bool;
            let mut user_data = "".to_string();

            if let Some(i) = matching.get(2) {
                user_data = i.to_owned().get_string();
                is_auth = matching[1].get_string() == "auth"
            } else {
                is_auth = false
            };
            let value = i.get_list()[1].get_string();
            hashmap.insert(route, (value, is_auth, user_data));
        }

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    self.stack
                        .push(Type::String(format!("{:?}", stream.peer_addr().unwrap())));
                    self.handle(stream, hashmap.clone(), buffer_size)
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
    }
}

/// To processing
fn parse_request_line(request_line: &str, key: &str) -> (String, String) {
    let parts: Vec<&str> = request_line.trim().split(key).collect();
    let method = parts.get(0).unwrap_or(&"").to_string();
    let path = parts.get(1).unwrap_or(&"").to_string();

    (method, path)
}

// Basic user authenticate
fn authenticate(request_str: &str, database: HashMap<String, String>) -> (bool, (String, String)) {
    let lines = request_str.lines();
    for line in lines {
        if line.starts_with("Authorization: Basic ") {
            // Decode string in the request
            let encoded_credentials = line.trim_start_matches("Authorization: Basic ");
            let decoded_credentials = base64::decode(encoded_credentials).unwrap_or_default();
            let credentials = String::from_utf8_lossy(&decoded_credentials);

            // authenticate username and password
            let mut parts = credentials.splitn(2, ':');
            if let (Some(username), Some(password)) = (parts.next(), parts.next()) {
                if let Some(expected_password) = database.get(username) {
                    return (
                        password == expected_password,
                        (username.to_string(), password.to_string()),
                    );
                }
            }
        }
    }
    (false, ("".to_string(), "".to_string()))
}

// Execute SQL query and return table data
fn sql(db_path: &str, sql_query: &str) -> Type {
    let conn = match Connection::open(db_path) {
        Ok(connection) => connection,
        Err(_) => return Type::Error("sql-connect".to_string()),
    };

    // preprocessing to execution query
    let mut stmt = match conn.prepare(sql_query) {
        Ok(statement) => statement,
        Err(_) => return Type::Error("pre-query".to_string()),
    };

    // Get table's rows
    let rows = match stmt.query_map([], |row| {
        let result: Result<Vec<(String, Type)>, rusqlite::Error> = Ok((0..row.column_count())
            .map(|index| {
                let column = row.column_name(index).unwrap().to_string();
                let value = {
                    let value = row.get_raw(index);
                    if let Ok(i) = value.as_str() {
                        Type::String(i.to_string())
                    } else {
                        if let Ok(i) = value.as_i64() {
                            Type::Number(i as f64)
                        } else {
                            if let Ok(i) = value.as_f64() {
                                Type::Number(i)
                            } else {
                                Type::Error("parse-db".to_string())
                            }
                        }
                    }
                };
                (column, value)
            })
            .collect());
        result
    }) {
        Ok(rows) => rows,
        Err(_) => return Type::Error("exe-query".to_string()),
    };

    // Parse type for Stack
    let mut result = Vec::new();
    for row in rows {
        match row {
            Ok(values) => result.push({
                let mut object = HashMap::new();
                for (property, value) in values {
                    object.insert(property, value);
                }
                Type::Object("table".to_string(), object)
            }),
            Err(_) => return Type::List(vec![]),
        }
    }

    // Return table as list
    Type::List(result)
}
