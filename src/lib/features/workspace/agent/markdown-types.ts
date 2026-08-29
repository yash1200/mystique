export type MdInline =
	| { type: 'text'; value: string }
	| { type: 'code'; value: string }
	| { type: 'strong'; children: MdInline[] }
	| { type: 'em'; children: MdInline[] }
	| { type: 'del'; children: MdInline[] }
	| { type: 'link'; href: string; children: MdInline[] }
	| { type: 'break' }

export type MdListItem = { children: MdBlock[] }

export type MdBlock =
	| { type: 'p'; children: MdInline[] }
	| { type: 'h'; level: 1 | 2 | 3; children: MdInline[] }
	| { type: 'code'; lang: string; value: string }
	| { type: 'ul'; items: MdListItem[] }
	| { type: 'ol'; start: number; items: MdListItem[] }
	| { type: 'blockquote'; children: MdBlock[] }
	| { type: 'table'; head: MdInline[][]; rows: MdInline[][][] }
	| { type: 'hr' }
