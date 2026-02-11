import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { Menu } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  NavigationMenu,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
} from '@/components/ui/navigation-menu';
import { Sheet, SheetContent, SheetTrigger } from '@/components/ui/sheet';
import { useAuth } from '@/contexts/auth-context';

const navLinks = [
  { textKey: 'nav.contestInfo', href: '#' },
  { textKey: 'nav.problems', href: '/problems' },
  { textKey: 'nav.submissions', href: '#' },
  { textKey: 'nav.ranking', href: '#' },
];

export function Navbar() {
  const { t } = useTranslation();
  const { user, logout } = useAuth();

  return (
    <header className="sticky top-8 z-50 -mb-4 px-4 pb-4 -translate-y-8">
      <div className="fade-bottom bg-background/15 absolute left-0 h-16 w-full backdrop-blur-lg" />
      <div className="max-w-container relative mx-auto">
        <div className="flex h-16 items-center justify-between">
          <div className="flex items-center gap-6">
            <Slot name="navbar.brand" as="div" />
            <NavigationMenu>
              <NavigationMenuList className="hidden md:flex">
                {navLinks.map((link) => (
                  <NavigationMenuItem key={link.textKey}>
                    <NavigationMenuLink
                      href={link.href}
                      className="px-3 py-2 text-sm hover:text-primary"
                    >
                      {t(link.textKey)}
                    </NavigationMenuLink>
                  </NavigationMenuItem>
                ))}
                <Slot name="navbar.menu" as="div" />
              </NavigationMenuList>
            </NavigationMenu>
          </div>
          <div className="flex items-center gap-4">
            {user ? (
              <>
                <span className="hidden text-sm md:block">
                  {user.username}
                </span>
                <Button variant="outline" onClick={logout}>
                  {t('auth.logout')}
                </Button>
              </>
            ) : (
              <>
                <a
                  href="/login"
                  className="hidden text-sm md:block"
                >
                  {t('nav.signIn')}
                </a>
                <Button variant="default" asChild>
                  <a href="/register">{t('nav.signUp')}</a>
                </Button>
              </>
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
                  <span className="sr-only">{t('nav.toggleMenu')}</span>
                </Button>
              </SheetTrigger>
              <SheetContent side="right">
                <nav className="grid gap-6 text-lg font-medium">
                  <a
                    href="#"
                    className="flex items-center gap-2 text-xl font-bold"
                  >
                    <span>{t('app.name')}</span>
                  </a>
                  {navLinks.map((link) => (
                    <a
                      key={link.textKey}
                      href={link.href}
                      className="text-muted-foreground hover:text-foreground"
                    >
                      {t(link.textKey)}
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
  );
}
