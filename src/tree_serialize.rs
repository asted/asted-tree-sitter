use super::message_generated::asted::interface::tree::{Location, Node, NodeArgs};
use flatbuffers::{self, WIPOffset};
use tree_sitter;

pub fn serialize(text: &[u16], tree: &tree_sitter::Tree) -> Vec<u8> {
	let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(1024);

	// TODO(sauyon): probably convert this into an iterative DFS instead of recursing
	let root_node = build_node(text, &mut builder, tree.root_node());

	builder.finish_size_prefixed(root_node, None);
	builder.finished_data().to_vec()
}

fn build_node<'a>(
	text: &[u16],
	builder: &mut flatbuffers::FlatBufferBuilder<'a>,
	node: tree_sitter::Node<'a>,
) -> WIPOffset<Node<'a>> {
	let kind = builder.create_string(node.kind());
	let location = Location::new(node.start_byte() as u64, node.end_byte() as u64);
	let child_vec = node.children(&mut node.walk()).map(|child| build_node(text, builder, child)).collect::<Vec<_>>();
	let children = builder.create_vector(&child_vec);

	let text = if child_vec.len() == 0 {
		Some(builder.create_vector(text))
	} else {
		None
	};

	Node::create(
		builder,
		&NodeArgs {
			kind: Some(kind),
			location: Some(&location),
			children: Some(children),
			named: node.is_named(),
			text: text,
		},
	)
}
