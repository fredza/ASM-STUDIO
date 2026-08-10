;@titre Liste des nombres premiers (cours 11.5)
;@enonce Le projet du cours testait UN nombre saisi ; ici la boucle sur R12 essaie
;@enonce tous les candidats de 2 à 50. Il manque ce qu'on fait d'un premier :
;@enonce l'AFFICHER, un par ligne, et le compter.
;@enonce Ouvre la boîte « Sortie du programme » (bouton ▣ de l'en-tête de la console)
;@enonce pour lire les quinze nombres tels que le terminal les écrirait.
;@enonce R14 doit finir à 15 (le compte) et R15 à 328 (leur somme).
;@attendu r14 == 15
;@attendu r15 == 328
;@attendu exit == 0

section .bss
    texte_nombre resb 8         ; de quoi écrire un nombre en chiffres

section .data
    saut db 10                  ; le retour à la ligne, « un par ligne »

section .text
    global _start

; afficher(RDI = adresse, RSI = longueur) — le write du chapitre 2, en fonction.
afficher:
    mov rdx, rsi                ; longueur
    mov rsi, rdi                ; adresse
    mov rax, 1                  ; syscall write
    mov rdi, 1                  ; stdout
    syscall
    ret

; afficher_nombre(RDI = nombre) — le convertit en chiffres, puis saute une ligne.
; Les divisions successives par 10 sortent les chiffres à l'envers : on remplit
; donc le tampon de la FIN vers le début.
afficher_nombre:
    mov rax, rdi
    mov r8, texte_nombre + 8    ; juste après le dernier octet
    xor rcx, rcx                ; nombre de chiffres écrits

.chiffre:
    dec r8
    xor rdx, rdx
    mov r9, 10
    div r9                      ; RAX = quotient, RDX = chiffre du bas
    add rdx, '0'                ; 7 → '7'
    mov [r8], dl
    inc rcx
    cmp rax, 0
    jnz .chiffre                ; il reste des chiffres à sortir

    mov rdi, r8                 ; début des chiffres écrits
    mov rsi, rcx
    call afficher
    mov rdi, saut
    mov rsi, 1
    call afficher
    ret

_start:
    xor r14, r14                ; combien de premiers trouvés
    xor r15, r15                ; leur somme
    mov r12, 2                  ; le candidat courant

.candidat:
    cmp r12, 50
    jg .fin                     ; on s'arrête après 50

    mov r13, 2                  ; diviseur d'essai

.diviseurs:
    mov rax, r13
    imul rax, r13
    cmp rax, r12
    jg .premier                 ; diviseur² > n : plus aucun diviseur possible

    mov rax, r12
    xor rdx, rdx
    div r13
    cmp rdx, 0
    je .suivant                 ; R13 divise R12 : pas premier

    inc r13
    jmp .diviseurs

.premier:
    ; TODO : compter ce premier      (« inc r14 »)
    ; TODO : l'ajouter à la somme    (« add r15, r12 »)
    ; TODO : l'afficher              (« mov rdi, r12 » puis « call afficher_nombre »)

.suivant:
    inc r12                     ; R12, R13, R14 et R15 survivent aux syscalls
    jmp .candidat

.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
