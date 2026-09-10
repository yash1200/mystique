export type MdInline =
	| { type: 'text'; value: string }
	| { type: 'code'; value: string }
	| { type: 'strong'; children: MdInline[] }
	| { type: 'em'; children: MdInline[] }
	| { type: 'del'; children: MdInline[] }
	| { type: 'link'; href: string; title?: string; children: MdInline[] }
	| { type: 'image'; src: string; alt: string; title?: string }
	| { type: 'math'; value: string }
	| { type: 'break' }

export type MdListItem = {
	children: MdBlock[]
	/** Present when the item is a GFM task (`- [ ]` / `- [x]`). */
	checked?: boolean
}

export type MdAlign = 'left' | 'center' | 'right'

export type MdAlertKind = 'note' | 'tip' | 'important' | 'warning' | 'caution'

export type MdBlock =
	| { type: 'p'; children: MdInline[] }
	| { type: 'h'; level: 1 | 2 | 3 | 4 | 5 | 6; children: MdInline[] }
	| { type: 'code'; lang: string; value: string }
	| { type: 'ul'; items: MdListItem[] }
	| { type: 'ol'; start: number; items: MdListItem[] }
	| { type: 'blockquote'; children: MdBlock[] }
	| {
			type: 'alert'
			kind: MdAlertKind
			children: MdBlock[]
	  }
	| {
			type: 'table'
			head: MdInline[][]
			rows: MdInline[][][]
			align: Array<MdAlign | null>
	  }
	| { type: 'math'; value: string }
	| { type: 'hr' }
