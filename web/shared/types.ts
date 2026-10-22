export type FieldKind = 'text' | 'number' | 'boolean' | 'date' | 'media' | 'relation'
export interface Field { key: string; label: string; kind: FieldKind; required: boolean }
export type Block =
  | { kind: 'paragraph'; text: string }
  | { kind: 'heading'; text: string; level: number }
  | { kind: 'image'; media_id: string; alt: string }
export interface Seo { description: string; canonical: string | null; noindex: boolean }
export interface ContentInput {
  content_type: string
  slug: string
  title: string
  blocks: Block[]
  fields: Record<string, string | number | boolean | null>
  extra_fields: Field[]
  seo: Seo
  status: 'draft' | 'published' | 'archived'
  publish_at: string
  version: number | null
}
export interface Content extends ContentInput {
  id: string
  author_id: string | null
  created_at: string
  updated_at: string
}
export type PublicContent = Pick<Content, 'id' | 'content_type' | 'slug' | 'title' | 'blocks' | 'fields' | 'seo' | 'publish_at' | 'updated_at'>
export interface ContentType { slug: string; name: string; fields: Field[] }
export interface Actor { identity_id: string | null; role: 'admin' | 'editor' | 'author' | 'agent'; label: string }
export interface NavItem { label: string; href: string; children: NavItem[] }
export interface Page<T> { items: T[]; limit: number; offset: number }
export interface Media { id: string; alt: string; width: number; height: number; variants: { width: number; url: string }[] }
export interface Revision { id: number; version: number; snapshot: Content; created_at: string }

export function errorMessage(error: unknown): string {
  if (error && typeof error === 'object' && 'data' in error) {
    const data = error.data
    if (data && typeof data === 'object' && 'error' in data) {
      const apiError = data.error
      if (apiError && typeof apiError === 'object' && 'message' in apiError && typeof apiError.message === 'string') return apiError.message
    }
  }
  return error instanceof Error ? error.message : 'Operazione non riuscita'
}
