; Multiboot2 header (used by GRUB, supports 64-bit ELF)
section .multiboot
align 8
MBOOT2_MAGIC        equ 0xE85250D6
MBOOT2_ARCH         equ 0  ; i386
MBOOT2_HEADER_LEN   equ multiboot2_end - multiboot2_start
MBOOT2_CHECKSUM     equ -(MBOOT2_MAGIC + MBOOT2_ARCH + MBOOT2_HEADER_LEN)

multiboot2_start:
    dd MBOOT2_MAGIC
    dd MBOOT2_ARCH
    dd MBOOT2_HEADER_LEN
    dd MBOOT2_CHECKSUM
    ; Tag: framebuffer request (type 5, flags=0, size=20)
    dd 5            ; type = framebuffer
    dd 0            ; flags = 0 (required)
    dd 20           ; size = 20
    dd 0            ; width = 0 (any)
    dd 0            ; height = 0 (any)
    dd 32           ; bpp = 32
    ; Tag: end (required)
    dw 0     ; type
    dw 0     ; flags
    dd 8     ; size
multiboot2_end:

; PVH ELF Note for direct QEMU loading of 64-bit ELF
XEN_ELFNOTE_PHYS32_ENTRY equ 18
section .note.Xen alloc
align 4
    dd 4             ; namesz = "Xen" + null
    dd 4             ; descsz = 4 bytes (32-bit entry)
    dd XEN_ELFNOTE_PHYS32_ENTRY
    db "Xen", 0      ; name (4 bytes)
    dd _start        ; 32-bit physical entry point

; --- Long mode page tables ---
section .data
align 4096
global _pml4
_pml4:
    times 512 dq 0

align 4096
_pdp:
    times 512 dq 0

align 4096
_pd:
    times 512 dq 0

align 4096
_pd_high:
    times 512 dq 0

; --- GDT for long mode ---
section .data
align 16
gdt:
    dq 0x0000000000000000          ; null
    dq 0x00209A0000000000          ; 64-bit kernel code (ring 0)
    dq 0x0000920000000000          ; 64-bit kernel data (ring 0)
    dq 0x0020FA0000000000          ; 64-bit user code (ring 3)
    dq 0x0000F20000000000          ; 64-bit user data (ring 3)
gdt_end:

gdt_ptr:
    dw gdt_end - gdt - 1
    dq gdt

; --- Long mode GDT selectors ---
KERNEL_CS equ 0x08
KERNEL_DS equ 0x10
USER_CS   equ 0x1B
USER_DS   equ 0x23

; --- Entry point ---
section .text
bits 32
global _start
extern kernel_main

_start:
    ; Quick serial output to confirm boot
    mov dx, 0x3F8
    mov al, 'E'
    out dx, al
    mov al, 'L'
    out dx, al
    mov al, '!'
    out dx, al

    mov esp, stack_top

    ; Save multiboot info
    mov [multiboot_magic], eax
    mov [multiboot_info], ebx

    ; Check for CPUID
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 1 << 21
    push eax
    popfd
    pushfd
    pop eax
    xor eax, ecx
    je .no_cpuid
    push ecx
    popfd

    ; Check for long mode
    mov eax, 0x80000001
    cpuid
    test edx, 1 << 29
    jz .no_long_mode

    ; Disable interrupts
    cli

    ; Set up page tables
    mov edi, _pml4
    mov cr3, edi

    ; Zero PML4, PDP, PD
    mov edi, _pml4
    xor eax, eax
    mov ecx, 4096 / 4
    rep stosd

    mov edi, _pdp
    mov ecx, 4096 / 4
    rep stosd

    mov edi, _pd
    mov ecx, 4096 / 4
    rep stosd

    mov edi, _pd_high
    mov ecx, 4096 / 4
    rep stosd

    ; Identity map first 4GB with 2MB pages
    ; PML4[0] = _pdp + 0x03 (present, writable)
    mov eax, _pdp
    or eax, 0x03
    mov [_pml4], eax

    ; PDP[0] = _pd + 0x03 (present, writable)
    mov eax, _pd
    or eax, 0x03
    mov [_pdp], eax

    ; Also map at higher half (PML4[256] for 0xFFFF800000000000)
    mov eax, _pdp
    or eax, 0x03
    mov [_pml4 + 256*8], eax

    ; Fill PD: 512 entries * 2MB = 1GB identity map
    ; Map two PD tables for 2GB total
    mov edi, _pd
    xor esi, esi
    xor ecx, ecx
.identity_loop:
    mov eax, esi
    or eax, 0x83           ; present + writable + huge (2MB)
    mov [edi + ecx*8], eax
    add esi, 0x200000      ; 2MB
    inc ecx
    cmp ecx, 512
    jb .identity_loop

    ; Map second GB for framebuffer etc.
    mov eax, _pd_high
    or eax, 0x03
    mov [_pdp + 8], eax    ; PDP[1] = _pd_high

    mov edi, _pd_high
    mov esi, 0x40000000    ; start at 1GB
    xor ecx, ecx
.map_high:
    mov eax, esi
    or eax, 0x83
    mov [edi + ecx*8], eax
    add esi, 0x200000
    inc ecx
    cmp ecx, 512
    jb .map_high

    ; Enable PAE (5) only — SMEP/SMAP set later from Rust after boot
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    ; Enable long mode + NX
    mov ecx, 0xC0000080    ; EFER MSR
    rdmsr
    or eax, (1 << 8) | (1 << 11)   ; LME | NXE
    wrmsr

    ; Load 64-bit GDT (before enabling paging so KVM handles transition cleanly)
    lgdt [gdt_ptr]

    ; Enable paging
    mov eax, cr0
    or eax, 1 << 31        ; PG
    mov cr0, eax

    ; Far jump to 64-bit code
    jmp KERNEL_CS:.long_mode

bits 64
.long_mode:
    ; Set up segment registers
    mov ax, KERNEL_DS
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    mov rsp, stack_top_64

    cld                    ; clear direction flag per x86-64 ABI requirement

    ; Call kernel_main(uint32_t magic, uint32_t info)
    mov edi, [multiboot_magic]
    mov esi, [multiboot_info]
    call kernel_main

    cli
.hang:
    hlt
    jmp .hang

bits 32
.no_cpuid:
    mov esi, msg_no_cpuid
    call print_32
    jmp .hang

.no_long_mode:
    mov esi, msg_no_lm
    call print_32
    jmp .hang

; Simple 32-bit serial print
print_32:
    mov edx, 0x3F8
.loop:
    lodsb
    test al, al
    jz .done
    add edx, 0   ; dummy
    mov dx, 0x3F8
    out dx, al
    jmp .loop
.done:
    ret

section .rodata
msg_no_cpuid: db "CPU does not support CPUID", 10, 0
msg_no_lm:    db "CPU does not support long mode (x86-64)", 10, 0

section .bss
align 16
stack_bottom:
    resb 16384
stack_top:

stack_bottom_64:
    resb 16384
stack_top_64:

multiboot_magic: resd 1
multiboot_info: resd 1
