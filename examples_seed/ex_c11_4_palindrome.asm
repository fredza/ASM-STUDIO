;@titre Palindrome insensible à la casse (cours 11.4)
;@enonce Le mot testé est « Radar » : à l'endroit et à l'envers c'est le même mot,
;@enonce mais 'R' (82) et 'r' (114) diffèrent de 32, et la comparaison échoue.
;@enonce Convertis chaque lettre en minuscule avant de comparer.
;@enonce R14 doit finir à 1 (palindrome).
;@attendu r14 == 1
;@attendu exit == 0

section .data
    mot     db "Radar"
    mot_len equ $ - mot

section .text
    global _start

; minuscule(DIL) → DIL en minuscule. Une majuscule est comprise entre 'A' et 'Z' ;
; ajouter 32 donne la minuscule correspondante. Tout le reste passe intact.
minuscule:
    cmp dil, 'A'
    jb .fait
    cmp dil, 'Z'
    ja .fait
    ; TODO : passer en minuscule  (« add dil, 32 »)
.fait:
    ret

_start:
    xor rcx, rcx                ; indice depuis le début
    mov rdx, mot_len
    dec rdx                     ; indice depuis la fin

.boucle:
    cmp rcx, rdx
    jge .palindrome             ; les deux curseurs se sont croisés

    movzx rdi, byte [mot + rcx]
    call minuscule
    mov r8b, dil                ; lettre de gauche, normalisée

    movzx rdi, byte [mot + rdx]
    call minuscule              ; lettre de droite, normalisée

    cmp r8b, dil
    jne .pas_palindrome

    inc rcx
    dec rdx
    jmp .boucle

.palindrome:
    mov r14, 1                  ; R14 survit au « mov rax, 60 » de la sortie
    jmp .fin

.pas_palindrome:
    xor r14, r14

.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
