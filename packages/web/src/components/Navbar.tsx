import { Menu } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  NavigationMenu,
  NavigationMenuList,
  NavigationMenuItem,
  NavigationMenuLink,
} from "@/components/ui/navigation-menu"
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet"
import { Slot } from "@broccoli/sdk/react"

const navLinks = [
  { text: "Contest Info", href: "#" },
  { text: "Problems", href: "#" },
  { text: "Submissions", href: "#" },
  { text: "Ranking", href: "#" },
]

const actions = [
  { text: "Sign in", href: "#", isButton: false },
  { text: "Sign up", href: "#", isButton: true },
]

export function Navbar() {
  return (
    <header className="sticky top-0 z-50 -mb-4 px-4 pb-4 -translate-y-8">
      <div className="fade-bottom bg-background/15 absolute left-0 h-24 w-full backdrop-blur-lg" />
      <div className="max-w-container relative mx-auto">
        <div className="flex h-16 items-center justify-between">
          <div className="flex items-center gap-6">
            <Slot name="navbar.brand" as="div" />
            <NavigationMenu>
              <NavigationMenuList className="hidden md:flex">
                {navLinks.map((link) => (
                  <NavigationMenuItem key={link.text}>
                    <NavigationMenuLink
                      href={link.href}
                      className="px-3 py-2 text-sm hover:text-primary"
                    >
                      {link.text}
                    </NavigationMenuLink>
                  </NavigationMenuItem>
                ))}
                <Slot name="navbar.menu" as="div" />
              </NavigationMenuList>
            </NavigationMenu>
          </div>
          <div className="flex items-center gap-4">
            {actions.map((action, index) =>
              action.isButton ? (
                <Button key={index} variant="default" asChild>
                  <a href={action.href}>{action.text}</a>
                </Button>
              ) : (
                <a
                  key={index}
                  href={action.href}
                  className="hidden text-sm md:block"
                >
                  {action.text}
                </a>
              )
            )}
            <Slot name="navbar.actions" as="div" />
            <Sheet>
              <SheetTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="shrink-0 md:hidden"
                >
                  <Menu className="size-5" />
                  <span className="sr-only">Toggle navigation menu</span>
                </Button>
              </SheetTrigger>
              <SheetContent side="right">
                <nav className="grid gap-6 text-lg font-medium">
                  <a href="#" className="flex items-center gap-2 text-xl font-bold">
                    <span>Broccoli OJ</span>
                  </a>
                  {navLinks.map((link) => (
                    <a
                      key={link.text}
                      href={link.href}
                      className="text-muted-foreground hover:text-foreground"
                    >
                      {link.text}
                    </a>
                  ))}
                  <Slot name="navbar.mobile.menu" as="div" />
                </nav>
              </SheetContent>
            </Sheet>
          </div>
        </div>
      </div>
    </header>
  )
}

