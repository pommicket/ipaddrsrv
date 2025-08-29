use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

#[derive(Debug)]
pub struct Args<Flag, Param> {
	flags: HashSet<Flag>,
	params: HashMap<Param, String>,
	lone_args: Vec<String>,
}

impl<Flag: Hash + Eq, Param: Hash + Eq> Args<Flag, Param> {
	pub fn is_set(&self, flag: Flag) -> bool {
		self.flags.contains(&flag)
	}
	pub fn get(&self, param: Param) -> Option<&str> {
		Some(self.params.get(&param)?.as_ref())
	}
	pub fn lone_args(&self) -> impl '_ + Iterator<Item = &str> {
		self.lone_args.iter().map(|x| x.as_ref())
	}
	pub fn lone_args_count(&self) -> usize {
		self.lone_args.len()
	}
}

pub fn parse_args<Flag: Copy + Hash + Eq + Debug, Param: Copy + Hash + Eq + Debug>(
	flag_names: &HashMap<&str, Flag>,
	param_names: &HashMap<&str, Param>,
) -> Result<Args<Flag, Param>, String> {
	let mut arg_iter = std::env::args_os();
	arg_iter.next(); // program name
	let mut args = Args {
		flags: HashSet::new(),
		params: HashMap::new(),
		lone_args: vec![],
	};
	let mut double_dash = false;
	let mut param: Option<(String, Param)> = None;
	assert!(flag_names.keys().all(|x| x.starts_with('-')));
	assert!(param_names.keys().all(|x| x.starts_with('-')));
	for arg in arg_iter {
		let arg = arg
			.into_string()
			.map_err(|arg| format!("Argument includes bad UTF-8: {arg:?}"))?;
		if let Some((_name, p)) = param.as_ref() {
			args.params.insert(*p, arg);
			param = None;
			continue;
		}
		if double_dash {
			args.lone_args.push(arg);
			continue;
		}
		if arg == "--" {
			double_dash = true;
			continue;
		}
		if let Some(flag) = flag_names.get(arg.as_str()) {
			args.flags.insert(*flag);
			continue;
		}
		if let Some(p) = param_names.get(arg.as_str()) {
			param = Some((arg, *p));
			continue;
		}
		if let Some((p, value)) = arg.split_once('=')
			&& let Some(p) = param_names.get(p)
		{
			args.params.insert(*p, value.into());
			continue;
		}
		if arg.starts_with('-') {
			return Err(format!("Unrecognized option: {arg}"));
		}
		args.lone_args.push(arg);
	}
	if let Some((name, _p)) = param {
		return Err(format!("No argument provided to {name}"));
	}
	Ok(args)
}
