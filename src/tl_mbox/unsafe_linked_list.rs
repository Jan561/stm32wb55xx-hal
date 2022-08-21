#[repr(C, packed)]
pub struct ListNode {
    next: *mut Self,
    prev: *mut Self,
}

pub unsafe fn init_head(list_head: *mut ListNode) {
    (*list_head).next = list_head;
    (*list_head).prev = list_head;
}

pub unsafe fn is_empty(list_head: *mut ListNode) -> bool {
    cortex_m::interrupt::free(|_| (*list_head).next == list_head)
}

pub unsafe fn insert_head(list_head: *mut ListNode, node: *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        (*node).next = (*list_head).next;
        (*node).prev = list_head;
        (*list_head).next = node;
        (*(*node).next).prev = node;
    });
}

pub unsafe fn insert_tail(list_head: *mut ListNode, node: *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        (*node).next = list_head;
        (*node).prev = (*list_head).prev;
        (*list_head).prev = node;
        (*(*node).prev).next = node;
    });
}

pub unsafe fn remove_node(node: *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        (*(*node).prev).next = (*node).next;
        (*(*node).next).prev = (*node).prev;
    });
}

pub unsafe fn remove_head(list_head: *mut ListNode, node: *mut *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        *node = (*list_head).next;
        remove_node((*list_head).next);
    });
}

pub unsafe fn remove_tail(list_head: *mut ListNode, node: *mut *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        *node = (*list_head).prev;
        remove_node((*list_head).prev);
    });
}

pub unsafe fn insert_node_after(node: *mut ListNode, ref_node: *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        (*node).next = (*ref_node).next;
        (*node).prev = ref_node;
        (*ref_node).next = node;
        (*(*node).next).prev = node;
    });
}

pub unsafe fn insert_node_before(node: *mut ListNode, ref_node: *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        (*node).next = ref_node;
        (*node).prev = (*ref_node).prev;
        (*ref_node).prev = node;
        (*(*node).prev).next = node;
    });
}

pub unsafe fn get_size(list_head: *mut ListNode) -> usize {
    cortex_m::interrupt::free(|_| {
        let mut size = 0;
        let mut temp = (*list_head).next;

        while temp != list_head {
            size += 1;
            temp = (*temp).next;
        }

        size
    })
}

pub unsafe fn get_next_node(ref_node: *mut ListNode, node: *mut *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        *node = (*ref_node).next;
    });
}

pub unsafe fn get_prev_node(ref_node: *const ListNode, node: *mut *mut ListNode) {
    cortex_m::interrupt::free(|_| {
        *node = (*ref_node).prev;
    });
}
