type MessageKind =
  | 'user'
  | 'agent'
  | 'tool_call'
  | 'tool_result'
  | 'system'
  | 'error'
  | 'permission'

type MessageStatus = 'streaming' | 'final' | 'pending' | 'approved' | 'denied'

type ChatMessage = {
  kind: MessageKind
  title: string
  content: string
  status?: MessageStatus
}

type Command =
  | { kind: 'reset' }
  | { kind: 'user_message'; content: string }
  | { kind: 'assistant_token'; token: string }
  | { kind: 'assistant_done'; content?: string }
  | { kind: 'tool_call'; name: string; args: unknown }
  | { kind: 'tool_result'; name: string; result: unknown }
  | { kind: 'system_message'; content: string }
  | { kind: 'error_message'; content: string }
  | { kind: 'permission_request'; name: string; args: unknown; reason: string }
  | { kind: 'permission_resolved'; name: string; decision: 'approved' | 'denied' }

type Response =
  | { action: 'noop' }
  | { action: 'append'; message: ChatMessage; tool_log_entry?: string }
  | { action: 'replace_last'; message: ChatMessage; tool_log_entry?: string }

class TranscriptControlPlane {
  private streamingAssistantRaw = ''
  private streamingAssistantVisible = false

  handle(command: Command): Response {
    switch (command.kind) {
      case 'reset':
        this.streamingAssistantRaw = ''
        this.streamingAssistantVisible = false
        return { action: 'noop' }
      case 'user_message':
        this.closeAssistantStream()
        return {
          action: 'append',
          message: {
            kind: 'user',
            title: 'Request',
            content: normalizeWhitespace(command.content),
          },
        }
      case 'assistant_token':
        this.streamingAssistantRaw += command.token
        return this.renderAssistantMessage('streaming', this.streamingAssistantRaw)
      case 'assistant_done': {
        const source = command.content ?? this.streamingAssistantRaw
        const response = this.renderAssistantMessage('final', source)
        this.closeAssistantStream()
        return response
      }
      case 'tool_call': {
        this.closeAssistantStream()
        const content = summarizeToolCall(command.name, command.args)
        return {
          action: 'append',
          message: {
            kind: 'tool_call',
            title: toolTitle(command.name),
            content,
          },
          tool_log_entry: firstLine(content),
        }
      }
      case 'tool_result': {
        const content = summarizeToolResult(command.name, command.result)
        return {
          action: 'append',
          message: {
            kind: 'tool_result',
            title: `${toolTitle(command.name)} Result`,
            content,
          },
          tool_log_entry: firstLine(content),
        }
      }
      case 'system_message':
        this.closeAssistantStream()
        return {
          action: 'append',
          message: {
            kind: 'system',
            title: 'Status',
            content: normalizeWhitespace(command.content),
          },
        }
      case 'error_message':
        this.closeAssistantStream()
        return {
          action: 'append',
          message: {
            kind: 'error',
            title: 'Error',
            content: normalizeWhitespace(command.content),
          },
        }
      case 'permission_request': {
        this.closeAssistantStream()
        return {
          action: 'append',
          message: {
            kind: 'permission',
            title: `Permission Required`,
            content: summarizePermissionRequest(
              command.name,
              command.args,
              command.reason,
            ),
            status: 'pending',
          },
        }
      }
      case 'permission_resolved':
        this.closeAssistantStream()
        return {
          action: 'append',
          message: {
            kind: 'permission',
            title:
              command.decision === 'approved'
                ? 'Permission Approved'
                : 'Permission Denied',
            content: `${toolTitle(command.name)} ${
              command.decision === 'approved' ? 'was approved.' : 'was denied.'
            }`,
            status: command.decision,
          },
        }
      default:
        return { action: 'noop' }
    }
  }

  private renderAssistantMessage(
    status: Extract<MessageStatus, 'streaming' | 'final'>,
    source: string,
  ): Response {
    const text = extractAssistantText(source)
    if (!text.trim()) {
      return { action: 'noop' }
    }

    const message: ChatMessage = {
      kind: 'agent',
      title: 'Response',
      content: text,
      status,
    }

    if (this.streamingAssistantVisible) {
      return { action: 'replace_last', message }
    }

    this.streamingAssistantVisible = status === 'streaming'
    return { action: 'append', message }
  }

  private closeAssistantStream() {
    this.streamingAssistantRaw = ''
    this.streamingAssistantVisible = false
  }
}

function normalizeWhitespace(text: string): string {
  return text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').trimEnd()
}

function firstLine(text: string): string {
  return normalizeWhitespace(text).split('\n')[0] ?? ''
}

function humanizeToolName(name: string): string {
  return name
    .split('_')
    .filter(Boolean)
    .map(part => part[0]?.toUpperCase() + part.slice(1))
    .join(' ')
}

function toolTitle(name: string): string {
  return humanizeToolName(name)
}

function summarizeToolCall(name: string, rawArgs: unknown): string {
  const args = asRecord(rawArgs)
  switch (name) {
    case 'edit_file': {
      const path = readString(args, 'file_path') || '<unknown file>'
      const preview = readBoolean(args, 'preview')
      const viaPatch = readString(args, 'patch').trim().length > 0
      const oldText = readString(args, 'old_string')
      const newText = readString(args, 'new_string')
      const editShape = viaPatch
        ? 'Apply a patch'
        : `Replace ${countLines(oldText)} line(s) with ${countLines(newText)} line(s)`
      return `${preview ? 'Preview' : 'Edit'} ${path}\n${editShape}`
    }
    case 'create_file': {
      const path = readString(args, 'path') || '<unknown file>'
      const content = readString(args, 'content')
      return `Create ${path}\n${countLines(content)} line(s), ${content.length} chars`
    }
    case 'read_file': {
      const path = readString(args, 'path') || '<unknown file>'
      const start = readNumber(args, 'start_line')
      const end = readNumber(args, 'end_line')
      if (start > 0 || end > 0) {
        return `Read ${path}\nLines ${start || 1}-${end || 'end'}`
      }
      return `Read ${path}`
    }
    case 'list_files': {
      const path = readString(args, 'path') || '.'
      const ext = readString(args, 'extension')
      return ext ? `List *.${ext} under ${path}` : `List files under ${path}`
    }
    case 'barq_search': {
      const query = readString(args, 'query')
      return `Search BARQ index\n${truncateInline(query, 120)}`
    }
    case 'cargo_check': {
      const dir = readString(args, 'dir') || '.'
      return `Run cargo check\nWorkspace: ${dir}`
    }
    case 'shell_exec': {
      const command = readString(args, 'command')
      const workingDir = readString(args, 'working_dir') || '.'
      return `Run shell command\n${command}\nDirectory: ${workingDir}`
    }
    case 'git_ops': {
      const operation = readString(args, 'operation')
      const extra = readString(args, 'args')
      return `Run git ${operation}${extra ? ` ${extra}` : ''}`
    }
    default: {
      const keys = Object.keys(args)
      return keys.length > 0
        ? `${toolTitle(name)}\nKeys: ${keys.join(', ')}`
        : toolTitle(name)
    }
  }
}

function summarizeToolResult(name: string, rawResult: unknown): string {
  const result = asRecord(rawResult)
  const explicitError = readString(result, 'error')
  if (explicitError) {
    return `${toolTitle(name)} failed\n${explicitError}`
  }

  switch (name) {
    case 'edit_file': {
      const path = readString(result, 'file_path') || '<unknown file>'
      if (readBoolean(result, 'preview')) {
        return `Preview ready for ${path}`
      }
      if (readBoolean(result, 'applied')) {
        return `Updated ${path}`
      }
      return `Edit request completed for ${path}`
    }
    case 'create_file': {
      const path = readString(result, 'path') || '<unknown file>'
      return readBoolean(result, 'created')
        ? `Created ${path}`
        : `Create request completed for ${path}`
    }
    case 'read_file': {
      const lineCount = readNumber(result, 'line_count')
      const sizeBytes = readNumber(result, 'size_bytes')
      return `Loaded file contents\n${lineCount} line(s), ${sizeBytes} bytes`
    }
    case 'list_files': {
      const files = Array.isArray(result.files) ? result.files.length : 0
      return `Listed ${files} file(s)`
    }
    case 'barq_search': {
      const matches = Array.isArray(result.results)
        ? result.results.length
        : readNumber(result, 'matches')
      return `BARQ search returned ${matches} match(es)`
    }
    case 'cargo_check': {
      const errors = Array.isArray(result.errors) ? result.errors.length : 0
      const warnings = Array.isArray(result.warnings) ? result.warnings.length : 0
      const state = readBoolean(result, 'success') ? 'passed' : 'reported issues'
      return `cargo check ${state}\n${errors} error(s), ${warnings} warning(s)`
    }
    case 'shell_exec': {
      const exitCode = readNumber(result, 'exit_code')
      const timedOut = readBoolean(result, 'timed_out')
      const stdout = readString(result, 'stdout')
      const stderr = readString(result, 'stderr')
      const preview = truncateMultiline(
        [stdout, stderr].filter(Boolean).join('\n').trim(),
        6,
        360,
      )
      const headline = timedOut
        ? `Shell command timed out (exit ${exitCode})`
        : `Shell command finished with exit ${exitCode}`
      return preview ? `${headline}\n${preview}` : headline
    }
    case 'git_ops': {
      const success = readBoolean(result, 'success')
      const output = truncateMultiline(readString(result, 'output'), 6, 360)
      const headline = success ? 'Git command completed' : 'Git command failed'
      return output ? `${headline}\n${output}` : headline
    }
    default: {
      const compact = JSON.stringify(result)
      return compact && compact !== '{}'
        ? `${toolTitle(name)} completed\n${truncateInline(compact, 240)}`
        : `${toolTitle(name)} completed`
    }
  }
}

function summarizePermissionRequest(
  name: string,
  rawArgs: unknown,
  reason: string,
): string {
  const summary = summarizeToolCall(name, rawArgs)
  return `${toolTitle(name)} needs approval\n${summary}\nReason: ${normalizeWhitespace(reason)}`
}

function extractAssistantText(raw: string): string {
  const trimmed = normalizeWhitespace(raw).trim()
  if (!trimmed) {
    return ''
  }

  const unfenced = stripFencedBlock(trimmed)
  const parsed = parseJson(unfenced)
  if (parsed !== undefined) {
    const extracted = collectPreferredText(parsed)
    if (extracted) {
      return extracted
    }
  }

  for (const field of ['final_answer', 'answer', 'message', 'content', 'text']) {
    const partial = extractJsonStringField(unfenced, field)
    if (partial) {
      return partial
    }
  }

  if (looksStructured(unfenced)) {
    return ''
  }

  return trimmed
}

function stripFencedBlock(raw: string): string {
  const match = raw.match(/^```(?:json|JSON)?\s*([\s\S]*?)\s*```$/)
  return match ? match[1] ?? raw : raw
}

function parseJson(raw: string): unknown | undefined {
  try {
    return JSON.parse(raw)
  } catch {
    return undefined
  }
}

function collectPreferredText(value: unknown): string {
  if (typeof value === 'string') {
    return normalizeWhitespace(value)
  }

  if (Array.isArray(value)) {
    const parts = value
      .map(item => collectPreferredText(item))
      .filter(part => part.trim().length > 0)
    return parts.join('\n\n').trim()
  }

  if (!value || typeof value !== 'object') {
    return ''
  }

  const record = value as Record<string, unknown>
  for (const key of ['final_answer', 'answer', 'message', 'content', 'text']) {
    const nested = collectPreferredText(record[key])
    if (nested) {
      return nested
    }
  }

  if (Array.isArray(record.content)) {
    const textBlocks = record.content
      .map(block => {
        const entry = asRecord(block)
        if (entry.type === 'text' && typeof entry.text === 'string') {
          return normalizeWhitespace(entry.text)
        }
        return ''
      })
      .filter(Boolean)
    if (textBlocks.length > 0) {
      return textBlocks.join('\n\n')
    }
  }

  if (record.message && typeof record.message === 'object') {
    const nested = collectPreferredText(record.message)
    if (nested) {
      return nested
    }
  }

  return ''
}

function extractJsonStringField(raw: string, field: string): string {
  const needle = `"${field}"`
  const fieldStart = raw.indexOf(needle)
  if (fieldStart < 0) {
    return ''
  }

  const afterField = raw.slice(fieldStart + needle.length)
  const colonIndex = afterField.indexOf(':')
  if (colonIndex < 0) {
    return ''
  }

  let rest = afterField.slice(colonIndex + 1).trimStart()
  if (!rest.startsWith('"')) {
    return ''
  }

  rest = rest.slice(1)
  let out = ''
  let escaped = false
  for (const ch of rest) {
    if (escaped) {
      switch (ch) {
        case 'n':
          out += '\n'
          break
        case 'r':
          out += '\r'
          break
        case 't':
          out += '\t'
          break
        case '\\':
          out += '\\'
          break
        case '"':
          out += '"'
          break
        default:
          out += ch
          break
      }
      escaped = false
      continue
    }

    if (ch === '\\') {
      escaped = true
      continue
    }

    if (ch === '"') {
      break
    }

    out += ch
  }

  return normalizeWhitespace(out)
}

function looksStructured(raw: string): boolean {
  return raw.startsWith('{') || raw.startsWith('[')
}

function truncateInline(text: string, maxChars: number): string {
  if (text.length <= maxChars) {
    return text
  }
  return `${text.slice(0, Math.max(0, maxChars - 3))}...`
}

function truncateMultiline(text: string, maxLines: number, maxChars: number): string {
  const normalized = normalizeWhitespace(text)
  if (!normalized) {
    return ''
  }

  const lines = normalized.split('\n').slice(0, maxLines)
  let joined = lines.join('\n')
  if (joined.length > maxChars) {
    joined = `${joined.slice(0, Math.max(0, maxChars - 3))}...`
  } else if (normalized.split('\n').length > maxLines) {
    joined += '\n...'
  }
  return joined
}

function countLines(text: string): number {
  if (!text) {
    return 0
  }
  return normalizeWhitespace(text).split('\n').length
}

function asRecord(value: unknown): Record<string, any> {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, any>
  }
  return {}
}

function readString(record: Record<string, any>, key: string): string {
  const value = record[key]
  return typeof value === 'string' ? normalizeWhitespace(value) : ''
}

function readNumber(record: Record<string, any>, key: string): number {
  const value = record[key]
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function readBoolean(record: Record<string, any>, key: string): boolean {
  return record[key] === true
}

const controlPlane = new TranscriptControlPlane()

let buffer = ''
process.stdin.setEncoding('utf8')
process.stdin.on('data', chunk => {
  buffer += chunk
  for (;;) {
    const newline = buffer.indexOf('\n')
    if (newline < 0) {
      break
    }

    const line = buffer.slice(0, newline)
    buffer = buffer.slice(newline + 1)
    if (!line.trim()) {
      continue
    }

    let response: Response
    try {
      const command = JSON.parse(line) as Command
      response = controlPlane.handle(command)
    } catch (error) {
      response = {
        action: 'append',
        message: {
          kind: 'error',
          title: 'Control Plane Error',
          content:
            error instanceof Error ? error.message : 'Unknown control plane error',
        },
      }
    }

    process.stdout.write(JSON.stringify(response) + '\n')
  }
})
