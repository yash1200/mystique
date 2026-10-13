/** Chat state is per connection. Switching connections drops the previous chat id. */

export type ConnectionChatSnap = {
	connectionId: string | null
	chatId: string | null
	chats: unknown[]
	messages: unknown[]
}

export function chatAfterConnectionChange(
	prev: ConnectionChatSnap,
	nextConnectionId: string,
): ConnectionChatSnap {
	if (prev.connectionId === nextConnectionId) return prev
	return {
		connectionId: nextConnectionId,
		chatId: null,
		chats: [],
		messages: [],
	}
}
