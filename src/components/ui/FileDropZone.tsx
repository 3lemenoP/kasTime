import { useState, useCallback, DragEvent, ChangeEvent } from 'react'
import { Upload, Loader2 } from 'lucide-react'

interface FileDropZoneProps {
  onFileDrop: (file: File) => void
  accept?: string
  description?: string
  isLoading?: boolean
}

function FileDropZone({
  onFileDrop,
  accept,
  description = 'Drop file to compute hash locally',
  isLoading = false,
}: FileDropZoneProps) {
  const [isDragging, setIsDragging] = useState(false)

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(true)
  }, [])

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(false)
  }, [])

  const handleDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      setIsDragging(false)

      const files = e.dataTransfer.files
      if (files && files.length > 0) {
        onFileDrop(files[0])
      }
    },
    [onFileDrop]
  )

  const handleFileSelect = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => {
      const files = e.target.files
      if (files && files.length > 0) {
        onFileDrop(files[0])
      }
    },
    [onFileDrop]
  )

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={`
        relative border-2 border-dashed rounded-lg p-8
        transition-all duration-200 ease-out
        ${isDragging
          ? 'border-[var(--accent-primary)] bg-[var(--accent-primary-dim)]'
          : 'border-[var(--border-default)] bg-[var(--bg-tertiary)] hover:border-[var(--border-strong)]'
        }
      `}
    >
      <input
        type="file"
        accept={accept}
        onChange={handleFileSelect}
        className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
        disabled={isLoading}
      />

      <div className="flex flex-col items-center justify-center text-center">
        {isLoading ? (
          <>
            <Loader2 className="w-10 h-10 text-[var(--accent-primary)] mb-4 animate-spin" />
            <p className="text-sm text-[var(--text-secondary)]">Computing hash...</p>
          </>
        ) : (
          <>
            <div className={`
              w-16 h-16 rounded-lg flex items-center justify-center mb-4
              transition-colors duration-200
              ${isDragging ? 'bg-[var(--accent-primary)]' : 'bg-[var(--bg-quaternary)]'}
            `}>
              <Upload className={`
                w-8 h-8 transition-colors duration-200
                ${isDragging ? 'text-[var(--text-inverse)]' : 'text-[var(--text-tertiary)]'}
              `} />
            </div>
            <p className="text-sm font-medium text-[var(--text-primary)] mb-1">
              {isDragging ? 'Drop to upload' : 'Drop file or click to upload'}
            </p>
            <p className="text-xs text-[var(--text-tertiary)]">{description}</p>
            <p className="text-xs text-[var(--text-tertiary)] mt-2">
              File never leaves your device
            </p>
          </>
        )}
      </div>
    </div>
  )
}

export default FileDropZone
