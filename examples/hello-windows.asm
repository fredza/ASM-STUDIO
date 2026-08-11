; hello-windows.asm — un programme Windows, assemblé depuis Linux
;
; Choisissez la cible « Windows — PE64 console » (menu Exécution ▸ Cible), puis
; assemblez (Ctrl+B). ASM Studio produit un vrai .exe : le panneau FORMAT montre
; ses sections, son point d'entrée et sa table d'import, et le panneau
; DÉSASSEMBLAGE son code machine. Il ne s'exécutera pas ici — il n'y a pas de
; Windows sous la main — mais il se lance tel quel sur une machine Windows.
;
; Trois différences avec un programme Linux, et elles sont toutes ici :
;
;  1. Pas de « syscall ». Windows ne se parle pas directement : on appelle des
;     fonctions de DLL (GetStdHandle, WriteFile, ExitProcess), déclarées
;     « extern ». Le lieur les inscrit dans la table d'import du .exe.
;
;  2. Autre convention d'appel. Les quatre premiers arguments passent par
;     RCX, RDX, R8, R9 (et non RDI, RSI, RDX, RCX comme sous Linux) ; les
;     suivants vont sur la pile.
;
;  3. L'espace d'ombre (« shadow space »). Avant tout appel, il faut réserver
;     32 octets sur la pile pour l'appelé, même s'il n'en veut pas. Les 40
;     octets soustraits ici, ce sont ces 32 + 8 pour garder RSP aligné sur 16.

bits 64
default rel                             ; adressage relatif à RIP, comme sous Windows

section .data
    msg     db "Bonjour depuis un PE64 !", 13, 10   ; 13, 10 = retour chariot + saut de ligne
    msglen  equ $ - msg

section .bss
    ecrits  resq 1                      ; WriteFile y déposera le nombre d'octets écrits

section .text
    global main
    extern GetStdHandle
    extern WriteFile
    extern ExitProcess

main:
    sub     rsp, 40                     ; 32 d'espace d'ombre + 8 d'alignement

    mov     ecx, -11                    ; STD_OUTPUT_HANDLE
    call    GetStdHandle                ; → RAX = descripteur de la sortie

    mov     rcx, rax                    ; 1er argument : le descripteur
    lea     rdx, [msg]                  ; 2e : l'adresse du texte
    mov     r8d, msglen                 ; 3e : combien d'octets
    lea     r9, [ecrits]                ; 4e : où ranger le compte écrit
    mov     qword [rsp + 32], 0         ; 5e argument : sur la pile, après l'ombre
    call    WriteFile

    xor     ecx, ecx                    ; code de sortie 0
    call    ExitProcess                 ; ne revient jamais
