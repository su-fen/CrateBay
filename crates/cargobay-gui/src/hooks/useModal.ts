import { useState, useCallback } from "react"
import type { ModalKind } from "../types"

export function useModal(t: (key: string) => string) {
  const [modalKind, setModalKind] = useState<ModalKind>("")
  const [modalTitle, setModalTitle] = useState("")
  const [modalBody, setModalBody] = useState("")
  const [modalCopyText, setModalCopyText] = useState("")
  const [packageContainer, setPackageContainer] = useState("")
  const [packageTag, setPackageTag] = useState("")
  const [packageLoading, setPackageLoading] = useState(false)

  const openTextModal = useCallback((title: string, body: string, copyText?: string) => {
    setModalKind("text")
    setModalTitle(title)
    setModalBody(body)
    setModalCopyText(copyText ?? body)
  }, [])

  const openPackageModal = useCallback((container: string, defaultTag: string) => {
    setModalKind("package")
    setModalTitle(t("packageImage"))
    setPackageContainer(container)
    setPackageTag(defaultTag)
    setModalBody(`${t("packageFromContainer")}\n${t("container")}: ${container}`)
    setModalCopyText("")
  }, [t])

  const closeModal = useCallback(() => {
    setModalKind("")
    setModalTitle("")
    setModalBody("")
    setModalCopyText("")
    setPackageContainer("")
    setPackageTag("")
    setPackageLoading(false)
  }, [])

  return {
    modalKind, modalTitle, modalBody, modalCopyText,
    packageContainer, packageTag, setPackageTag,
    packageLoading, setPackageLoading,
    openTextModal, openPackageModal, closeModal,
  }
}
