;@titre Tri décroissant (cours 11.2)
;@enonce Ce tri à bulles range le tableau du plus PETIT au plus grand.
;@enonce Une seule comparaison sépare les deux sens : inverse-la pour trier du
;@enonce plus grand au plus petit. À la fin, R14 prend le premier élément et R15
;@enonce le dernier — donc R14 doit valoir 42 et R15 3.
;@attendu r14 == 42
;@attendu r15 == 3
;@attendu exit == 0

section .data
    t  dq 17, 3, 42, 8, 25
    n  equ ($ - t) / 8          ; 5 éléments

section .text
    global _start

_start:
    mov r12, n
    dec r12                     ; nombre de passes

.passe:
    cmp r12, 0
    jle .trie

    xor rcx, rcx                ; indice courant

.paire:
    cmp rcx, r12
    jge .fin_passe

    mov rax, [t + rcx*8]
    mov rbx, [t + rcx*8 + 8]

    ; TODO : inverser le sens du tri. « jle » garde l'ordre croissant :
    ; TODO : quel saut laisse les GRANDS devant ?
    cmp rax, rbx
    jle .pas_echange            ; déjà dans le bon ordre

    mov [t + rcx*8], rbx        ; on échange les deux voisins
    mov [t + rcx*8 + 8], rax

.pas_echange:
    inc rcx
    jmp .paire

.fin_passe:
    dec r12
    jmp .passe

.trie:
    ; R14 et R15 plutôt que RAX et RBX : le « mov rax, 60 » de la sortie écraserait
    ; le résultat juste avant qu'on puisse le lire.
    xor rcx, rcx
    mov r14, [t + rcx*8]        ; premier élément
    mov rcx, n - 1
    mov r15, [t + rcx*8]        ; dernier élément

    mov rax, 60
    xor rdi, rdi
    syscall
